//! Borrow-collision checking: rejects structural mutation of an object while a live
//! view into it (iterator cursor or `Span<T>`) exists in the same method body.
//!
//! Model:
//! - **Opening** — `x.iterator()`, `Span.of(x)`, and for-each loops over `x` open a borrow on
//!   the canonical receiver `x` (a local, or a one-level field chain rooted at `this`).
//!   Methods whose bodies return such a view of their receiver/parameter (B2 summaries,
//!   computed by [`super::receiver_modes`]-style inference) propagate borrows to their call
//!   sites: `let cur = get_view(list)` borrows `list`.
//! - **Liveness** — a cursor is live from construction to its *last textual reference* in the
//!   body (reassignment kills it early; end-of-body otherwise). For-each borrows close at the
//!   loop's closing brace. Cursor aliases (`let c2 = cur;`) extend liveness.
//! - **Violation** — while a borrow on `R` is live, any call of a `Unique`-mode method with
//!   canonical receiver `R`. Calls whose mode is unknown (external types, unresolved
//!   generics) are allowed: misses degrade to today's behavior, never false errors.
//!
//! Deliberately out of scope (documented): aliasing through re-binding (`let alias = list;`
//! then `alias.sort()`), cross-function borrows not covered by B2 summaries, and views stored
//! in fields. ARC keeps all of these memory-safe; this pass is a logic-bug preventer.

use super::*;
use crate::function_table::{FunctionIdentity, FunctionTable};
use dream_syntax::nodes::function::ReceiverMode;
use dream_syntax::nodes::statement::StatementNode;
use dream_text::text_span::TextSpan;
use dream_types::{TypeCtx, TypeId};
use indexmap::IndexSet as HashSet;

/// Flat events emitted in source order by the structural walk; interpreted afterwards.
#[derive(Debug, Clone)]
enum Ev {
    /// A view construction assigned to `cursor` borrowing `underlying`.
    Open {
        cursor: String,
        underlying: String,
        span: TextSpan,
    },
    /// Any reference to a name (read/write/call-receiver).
    Ref { name: String },
    /// `let to = from;` — `to` becomes a second live cursor bound to the same underlying.
    Alias { from: String, to: String },
    /// Rebinding of a name to a fresh value (kills prior cursor bindings).
    Rebind { name: String },
    /// Method call on canonical receiver `recv` named `name`.
    UniqueCandidate {
        recv: String,
        name: String,
        span: TextSpan,
    },
    /// `let to = from;` / `to = from;` where `from` is an object-typed name — the two names
    /// may reference the same instance (proven-alias edge for group tracking).
    /// For-each loop opens an anonymous borrow on the iterable for its body.
    ScopedOpen { underlying: String, span: TextSpan },
    /// End of a for-each body: the anonymous borrow dies.
    ScopedClose,
}

/// Canonical receiver key for a call receiver expression, given current aliases-of-this.
/// Returns `None` for receivers we cannot key (literals, complex chains, unknown locals).
fn is_self_expr(expr: &ExpressionNode, aliases: &[String]) -> bool {
    match expr {
        ExpressionNode::Identifier(t) => t.text == "this" || aliases.contains(&t.text),
        _ => false,
    }
}

/// Builds a dotted-chain key from any member-access expression rooted at an identifier.
/// `o.inner.items` -> `"o.inner.items"`; non-member roots return the expression's own key.
fn canonical_chain_from(expr: &ExpressionNode) -> Option<String> {
    match expr {
        ExpressionNode::Identifier(t) => Some(t.text.clone()),
        ExpressionNode::MemberAccess(base, member) => {
            let base_key = canonical_chain_from(base)?;
            Some(format!("{base_key}.{}", member.text))
        }
        ExpressionNode::Parenthesized(_, inner) => canonical_chain_from(inner),
        _ => None,
    }
}

/// True when the expression constructs a view over `recv_expr`: `.iterator()` calls,
/// `Span.of(x)` statics, and `Span<T>(x, ...)` constructor forms.
fn view_construction<'e, 'a>(
    e: &'e ExpressionNode<'a>,
) -> Option<(ViewKind, &'e ExpressionNode<'a>)> {
    match e {
        ExpressionNode::MethodCall(recv, name, _, args) => {
            if name.text == "iterator" && args.is_empty() {
                return Some((ViewKind::Iterator, recv));
            }
            None
        }
        ExpressionNode::FunctionCall(callee, _, args) => {
            if callee.text == "Span" {
                let first = args.first()?;
                return Some((ViewKind::Span, first));
            }
            None
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
enum ViewKind {
    Iterator,
    Span,
}

/// A method/free-function whose body returns a view of its receiver (methods) or of one of its
/// parameters (free functions): calling it opens a borrow on that receiver/argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewSource {
    /// The returned view points into the method's own receiver.
    Receiver,
    /// The returned view points into argument #n (0-based).
    Param(usize),
}

struct ViewSummary {
    source: ViewSource,
}

struct Extractor<'s> {
    summaries: &'s HashMap<FunctionIdentity, ViewSummary>,
    functions: &'s FunctionTable,
    type_ctx: &'s TypeCtx,
    struct_table: &'s StructTable,
    local_types: &'s IndexMap<String, TypeId>,
    // Lexical walker hints are not semantic type facts.
    class_names: &'s HashSet<String>,
    /// Owner class of the method being walked ("this").
    _owner: Option<TypeId>,
    aliases_this: Vec<String>,
    events: Vec<Ev>,
    local_class: Vec<(String, String)>,
}

impl<'s> Extractor<'s> {
    fn recv_key(&mut self, expr: &ExpressionNode) -> Option<String> {
        canonical_chain_from(expr)
    }

    /// Kills any prior binding of `name`, walks the initializer (so nested calls/refs are seen),
    /// then opens/aliases when the initializer produces a view.
    fn emit_binding_and_init(
        &mut self,
        name: &str,
        init: &ExpressionNode,
        field_types: &indexmap::IndexMap<String, String>,
        class_fields: &[String],
    ) {
        self.events.push(Ev::Rebind {
            name: name.to_string(),
        });
        if let Some((_kind, recv_expr)) = view_construction(init) {
            if let (Some(underlying), Some(span)) = (self.recv_key(recv_expr), init_span(init)) {
                self.events.push(Ev::Open {
                    cursor: name.to_string(),
                    underlying,
                    span,
                });
            }
            return;
        }
        if let ExpressionNode::Identifier(src) = init {
            self.events.push(Ev::Alias {
                from: src.text.clone(),
                to: name.to_string(),
            });
            return;
        }
        // B2 summary call: `let cur = get_view(list);` / `let v = obj.view();`
        let candidates = match init {
            ExpressionNode::FunctionCall(callee, _, _) => {
                self.functions.candidates(self.type_ctx, &callee.text)
            }
            ExpressionNode::MethodCall(recv, name, _, _) => canonical_chain_from(recv)
                .and_then(|chain| {
                    chain_type(self.struct_table, self._owner, self.local_types, &chain)
                })
                .and_then(|owner| self.functions.methods.get(&(owner, name.text.clone())))
                .cloned()
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let summary = candidates
            .first()
            .and_then(|identity| self.summaries.get(identity))
            .filter(|first| {
                candidates.iter().all(|identity| {
                    self.summaries
                        .get(identity)
                        .is_some_and(|summary| summary.source == first.source)
                })
            });
        if let Some(summary) = summary {
            let borrowed: Option<&ExpressionNode> = match init {
                ExpressionNode::FunctionCall(_, _, args) => match summary.source {
                    ViewSource::Param(idx) => args.get(idx),
                    _ => None,
                },
                ExpressionNode::MethodCall(recv, _, _, args) => match summary.source {
                    ViewSource::Receiver => Some(recv),
                    ViewSource::Param(idx) => args.get(idx),
                },
                _ => None,
            };
            if let Some(borrowed) = borrowed {
                if let (Some(underlying), Some(span)) = (self.recv_key(borrowed), init_span(init)) {
                    self.events.push(Ev::Open {
                        cursor: name.to_string(),
                        underlying,
                        span,
                    });
                    return;
                }
            }
        }
        self.walk_expression_pub(init, field_types, class_fields);
    }

    /// Public shim so statement-level walkers can reuse the expression walker.
    fn walk_expression_pub(
        &mut self,
        e: &ExpressionNode,
        field_types: &indexmap::IndexMap<String, String>,
        class_fields: &[String],
    ) {
        self.walk_expr(e, field_types, class_fields);
    }
}

fn view_summary(function: &FunctionNode, is_method: bool) -> Option<ViewSummary> {
    let parameters: Vec<_> = function
        .parameters
        .iter()
        .filter(|parameter| !is_method || parameter.name.text != "this")
        .map(|parameter| parameter.name.text.as_str())
        .collect();
    for statement in function.body {
        let StatementNode::Return(Some(expression)) = statement else {
            continue;
        };
        let Some((_, receiver)) = view_construction(expression) else {
            continue;
        };
        let ExpressionNode::Identifier(name) = receiver else {
            continue;
        };
        let source = if is_method && name.text == "this" {
            Some(ViewSource::Receiver)
        } else {
            parameters
                .iter()
                .position(|parameter| *parameter == name.text)
                .map(ViewSource::Param)
        };
        if let Some(source) = source {
            return Some(ViewSummary { source });
        }
    }
    None
}

fn chain_type(
    structs: &StructTable,
    owner: Option<TypeId>,
    locals: &IndexMap<String, TypeId>,
    chain: &str,
) -> Option<TypeId> {
    let mut parts = chain.split('.');
    let root = parts.next()?;
    let mut ty = if root == "this" {
        owner?
    } else {
        *locals.get(root)?
    };
    for field in parts {
        ty = structs.get_struct(ty)?.fields.get(field)?.ty;
    }
    Some(ty)
}

/// Interprets the flat event stream with **group-keyed** tracking: names proven to reference
/// the same instance (assignment, view construction) share a group root, so a mutation through
/// one name conflicts with live views opened through any other name in the group.
fn interpret_events(
    events: &[Ev],
    analyzer: &Analyzer<'_>,
    owner: Option<TypeId>,
    local_types: &IndexMap<String, TypeId>,
    file_path: &Option<Rc<str>>,
    diagnostics: &mut DiagnosticBag,
) {
    use indexmap::IndexMap as StdHashMap;

    // --- Last-reference precompute: final index of each tracked name ----------------------
    let mut last_ref: StdHashMap<String, usize> = StdHashMap::new();
    for (idx, ev) in events.iter().enumerate() {
        let named: Option<&String> = match ev {
            Ev::Open { cursor, .. } => Some(cursor),
            Ev::Alias { to, .. } => Some(to),
            Ev::Ref { name } | Ev::Rebind { name } => Some(name),
            _ => None,
        };
        if let Some(name) = named {
            last_ref
                .entry(name.clone())
                .and_modify(|v| *v = (*v).max(idx))
                .or_insert(idx);
        }
    }
    let mut scoped: Vec<(String, TextSpan)> = Vec::new();
    let mut live_cursors: Vec<(String, String)> = Vec::new(); // (cursor, underlying key)

    // --- Alias groups --------------------------------------------------------------------
    // name -> group root. Groups merge on object aliasing; views record the GROUP ROOT of
    // their underlying so mutations through any group member are caught.
    // Alias groups: names assigned from each other share a group root. Views record the
    // group root of their underlying so mutations through any group member are caught.
    if std::env::var("DREAM_TRACE_BORROW").is_ok() {
        for (i, ev) in events.iter().enumerate() {
            eprintln!("[borrow {i}] {:?}", ev);
        }
    }
    let _ = std::env::var("DREAM_TRACE_BORROW");

    for (i, ev) in events.iter().enumerate() {
        match ev {
            Ev::Open {
                cursor, underlying, ..
            } => {
                live_cursors.retain(|(n, _)| n != cursor);
                live_cursors.push((cursor.clone(), underlying.clone()));
            }
            Ev::Alias { from, to } => {
                if let Some(u) = live_cursors
                    .iter()
                    .find(|(n, _)| n == from)
                    .map(|(_, u)| u.clone())
                {
                    live_cursors.retain(|(n, _)| n != to);
                    live_cursors.push((to.clone(), u));
                }
            }
            Ev::Ref { .. } | Ev::Rebind { .. } => {}
            Ev::ScopedOpen { underlying, span } => {
                let root = underlying.clone();
                scoped.push((root, *span));
            }
            Ev::ScopedClose => {
                scoped.pop();
            }
            Ev::UniqueCandidate {
                recv,
                name: callee,
                span,
            } => {
                let Some(ty) = chain_type(&analyzer.struct_table, owner, local_types, recv) else {
                    continue;
                };
                let unique = analyzer
                    .receiver_method_keys(ty, callee)
                    .iter()
                    .any(|key| analyzer.receiver_modes.get(key) == Some(&ReceiverMode::Unique));
                if !unique {
                    continue;
                }
                let recv_root = recv.clone();
                // Strong cursor on this group, still referenced later?
                let strong_hit = live_cursors
                    .iter()
                    .find(|(_, u)| u == &recv_root)
                    .and_then(|(n, _)| last_ref.get(n))
                    .map(|&last| last > i)
                    .unwrap_or(false);
                let scoped_hit = scoped.iter().any(|(u, _)| *u == recv_root);
                if std::env::var("DREAM_TRACE_BORROW").is_ok() {
                    eprintln!(
                        "[verdict] recv={recv} callee={callee} strong={strong_hit} scoped={scoped_hit} cursors={live_cursors:?}"
                    );
                }
                if strong_hit || scoped_hit {
                    let opened = scoped
                        .iter()
                        .find(|(u, _)| *u == recv_root)
                        .map(|(_, s)| *s)
                        .or_else(|| {
                            live_cursors
                                .iter()
                                .rev()
                                .find(|(n, u)| {
                                    u == &recv_root
                                        && last_ref.get(n).map(|&l| l > i).unwrap_or(false)
                                })
                                .and_then(|(cn, _)| {
                                    events.iter().find_map(|ev| match ev {
                                        Ev::Open { cursor, span, .. } if cursor == cn => {
                                            Some(*span)
                                        }
                                        _ => None,
                                    })
                                })
                        });
                    let opened_note = opened
                        .as_ref()
                        .map(|s| format!("view created at line {}", s.line_no))
                        .unwrap_or_else(|| "an earlier view".to_string());
                    if file_path.is_some() {
                        diagnostics.file_path = file_path_string(file_path);
                    }
                    diagnostics.report_error(
                        format!(
                            "cannot call '{callee}' here: it mutates the object while a live view into it exists ({opened_note}). Drop or finish using the view before mutating",
                        ),
                        Some(*span),
                    );
                }
            }
        }
    }
}

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn check_borrow_collisions(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let scope = self.type_ctx.scope();
        let mut summaries = HashMap::new();
        let functions: Vec<_> = self
            .struct_methods
            .iter()
            .map(|(function, _)| (*function, true))
            .chain(node.functions.iter().map(|function| (*function, false)))
            .chain(self.instantiated_generics.values().map(|(_, function)| {
                (
                    *function,
                    function
                        .parameters
                        .first()
                        .is_some_and(|parameter| parameter.name.text == "this"),
                )
            }))
            .collect();
        for (function, is_method) in &functions {
            if function.is_static || function.is_extern || function.body.is_empty() {
                continue;
            }
            self.type_ctx
                .set_scope(self.graph.module_for_file(function.file_path.as_deref()));
            if let (Some(identity), Some(summary)) = (
                self.function_declaration(function),
                view_summary(function, *is_method),
            ) {
                summaries.insert(identity, summary);
            }
        }

        for (function, is_method) in functions {
            if function.is_static || function.is_extern || function.body.is_empty() {
                continue;
            }
            self.type_ctx
                .set_scope(self.graph.module_for_file(function.file_path.as_deref()));
            let Some(identity) = self.function_declaration(function) else {
                continue;
            };
            let Some(hir) = self
                .hir
                .functions
                .iter()
                .find(|hir| hir.def == identity.0 && hir.instance == identity.1)
            else {
                continue;
            };
            let local_types: IndexMap<_, _> = hir
                .params
                .iter()
                .map(|parameter| (parameter.name.clone(), parameter.ty))
                .chain(
                    hir.locals
                        .iter()
                        .map(|local| (local.name.clone(), local.ty)),
                )
                .collect();
            let owner = if is_method {
                self.receiver_function_key(&identity).map(|(key, _)| key.0)
            } else {
                None
            };
            let fields: Vec<_> = owner
                .and_then(|ty| self.struct_table.get_struct(ty))
                .map(|info| info.fields.keys().cloned().collect())
                .unwrap_or_default();
            // The structural walker retains lexical hints, but semantic receiver identity
            // comes exclusively from the analyzed HIR locals and registered field types.
            let class_names = HashSet::new();
            let mut extractor = Extractor {
                summaries: &summaries,
                functions: &self.function_table,
                type_ctx: &self.type_ctx,
                struct_table: &self.struct_table,
                local_types: &local_types,
                class_names: &class_names,
                _owner: owner,
                aliases_this: Vec::new(),
                events: Vec::new(),
                local_class: Vec::new(),
            };
            extractor.walk_block(function.body, &IndexMap::new(), &fields);
            interpret_events(
                &extractor.events,
                self,
                owner,
                &local_types,
                &function.file_path,
                diagnostics,
            );
        }
        self.type_ctx.set_scope(scope);
    }
}

fn init_span(e: &ExpressionNode) -> Option<TextSpan> {
    match e {
        ExpressionNode::Identifier(t) => Some(t.position),
        ExpressionNode::MemberAccess(base, _) => init_span(base),
        ExpressionNode::FunctionCall(callee, ..) => Some(callee.position),
        ExpressionNode::MethodCall(_, name, ..) => Some(name.position),
        _ => None,
    }
}

mod traversal;
