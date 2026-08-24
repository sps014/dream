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
use dream_syntax::nodes::function::ReceiverMode;
use dream_syntax::nodes::statement::StatementNode;
use dream_text::text_span::TextSpan;

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
    /// For-each loop opens an anonymous borrow on the iterable for its body.
    ScopedOpen {
        underlying: String,
        span: TextSpan,
    },
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

fn type_owner_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Struct(token, _) => Some(token.text.clone()),
        Type::String(_) => Some("string".to_string()),
        Type::Integer(_) => Some("int".to_string()),
        Type::Float(_) => Some("float".to_string()),
        Type::Double(_) => Some("double".to_string()),
        Type::Boolean(_) => Some("bool".to_string()),
        Type::Byte(_) => Some("byte".to_string()),
        Type::Char(_) => Some("char".to_string()),
        Type::Long(_) => Some("long".to_string()),
        Type::UInt(_) => Some("uint".to_string()),
        Type::ULong(_) => Some("ulong".to_string()),
        _ => None,
    }
}

fn canonical_recv(
    expr: &ExpressionNode,
    aliases_this: &[String],
) -> Option<String> {
    match expr {
        ExpressionNode::Identifier(t) => {
            if t.text == "this" || aliases_this.contains(&t.text) {
                Some("this".to_string())
            } else {
                Some(t.text.clone())
            }
        }
        ExpressionNode::MemberAccess(base, member) => {
            let base_key = match &**base {
                ExpressionNode::Identifier(t) if t.text == "this" => "this".to_string(),
                _ => return None,
            };
            Some(format!("{base_key}.{}", member.text))
        }
        ExpressionNode::Parenthesized(_, inner) => canonical_recv(inner, aliases_this),
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
#[derive(Debug, Clone, Copy)]
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
    summaries: &'s HashMap<String, ViewSummary>,
    /// Names of declared classes — used to infer local types from constructor calls.
    class_names: &'s std::collections::HashSet<String>,
    /// Owner class of the method being walked ("this").
    _owner: String,
    aliases_this: Vec<String>,
    events: Vec<Ev>,
    /// Locals whose class was inferred from constructor calls / literals
    /// (`let m = Map<int, int>();` -> "Map"), used for callee-mode resolution.
    local_class: Vec<(String, String)>,
}

impl<'s> Extractor<'s> {
    /// Canonical key for a receiver we can track; also records local-class inference for
    /// constructor-initialized locals.
    fn recv_key(&mut self, expr: &ExpressionNode) -> Option<String> {
        match expr {
            ExpressionNode::Identifier(t) => {
                if t.text == "this" || self.aliases_this.contains(&t.text) {
                    Some("this".to_string())
                } else {
                    Some(t.text.clone())
                }
            }
            ExpressionNode::MemberAccess(base, member) => {
                let base_is_this =
                    matches!(&**base, ExpressionNode::Identifier(ref t) if t.text == "this");
                if base_is_this {
                    Some(format!("this.{}", member.text))
                } else {
                    None
                }
            }
            _ => None,
        }
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
            if let (Some(underlying), Some(span)) =
                (self.recv_key(recv_expr), init_span(init))
            {
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
        let summary = match init {
            ExpressionNode::FunctionCall(callee, _, _) => self.summaries.get(&callee.text),
            ExpressionNode::MethodCall(recv, nm, _, _) => {
                match &**recv {
                    ExpressionNode::Identifier(t) if t.text == "this" => {
                        let key = format!("{}::{}", self._owner, nm.text);
                        self.summaries.get(&key)
                    }
                    _ => None,
                }
            }
            _ => None,
        };
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
                if let (Some(underlying), Some(span)) =
                    (self.recv_key(borrowed), init_span(init))
                {
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

/// Computes B2 view-return summaries: `"Owner::method"` / `"fnname"` -> parameter position
/// whose argument gets borrowed (`usize::MAX` = the method receiver).
fn compute_view_summaries<'a>(node: &'a ProgramNode<'a>) -> HashMap<String, ViewSummary> {
    let mut out: HashMap<String, ViewSummary> = HashMap::new();

    /// Classifies a returned expression: does it construct a view over `this` (method) or over
    /// one of the function's parameters?
    fn view_return_source(
        e: &ExpressionNode,
        is_method: bool,
        param_positions: &[&str],
    ) -> Option<ViewSource> {
        let (_kind, recv) = view_construction(e)?;
        let ExpressionNode::Identifier(t) = recv else {
            return None;
        };
        if t.text == "this" && is_method {
            return Some(ViewSource::Receiver);
        }
        param_positions
            .iter()
            .position(|p| **p == t.text)
            .map(ViewSource::Param)
    }

    fn scan_fn(
        f: &FunctionNode,
        owner: Option<&str>,
        key: String,
        out: &mut HashMap<String, ViewSummary>,
    ) {
        if f.is_static || f.body.is_empty() {
            return;
        }
        let param_positions: Vec<&str> =
            f.parameters.iter().map(|p| p.name.text.as_str()).collect();
        for stmt in f.body {
            if let StatementNode::Return(Some(e)) = stmt {
                if let Some(source) =
                    view_return_source(e, owner.is_some(), &param_positions)
                {
                    out.insert(key.clone(), ViewSummary { source });
                    return;
                }
            }
        }
    }

    for s in node.structs.iter() {
        for m in &s.methods {
            let key = format!("{}::{}", s.name.text, m.name.text);
            scan_fn(m, Some(&s.name.text), key, &mut out);
        }
    }
    for e in node.extends.iter() {
        for m in &e.methods {
            let key = format!("{}::{}", e.target.text, m.name.text);
            scan_fn(m, Some(&e.target.text), key, &mut out);
        }
    }
    for f in node.functions.iter() {
        scan_fn(f, None, f.name.text.clone(), &mut out);
    }
    out
}

/// Interprets the flat event stream, reporting violations.
#[allow(clippy::too_many_arguments)]
fn interpret_events(
    events: &[Ev],
    owner: &str,
    receiver_modes: &HashMap<String, ReceiverMode>,
    field_types: &indexmap::IndexMap<String, String>,
    local_class: &[(String, String)],
    file_path: &Option<Rc<str>>,
    diagnostics: &mut DiagnosticBag,
) {
    use std::collections::HashMap as StdHashMap;

    // Precompute the LAST event index referencing each name over the whole stream — a cursor
    // is "still needed" at a mutation point iff its final reference comes later. Computing
    // this incrementally during the forward walk would miss future references entirely.
    let mut last_ref: StdHashMap<String, usize> = StdHashMap::new();
    let mut scoped: Vec<(String, TextSpan)> = Vec::new(); // (underlying, opened)
    let mut live: Vec<(String, String)> = Vec::new();

    if std::env::var("DREAM_TRACE_BORROW").is_ok() {
        for (i, ev) in events.iter().enumerate() {
            eprintln!("[borrow {i}] {:?}", ev);
        }
        eprintln!("local_class: {:?}", local_class);
    }
    let _ = std::env::var("DREAM_TRACE_BORROW");
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
    for (i, ev) in events.iter().enumerate() {
        match ev {
            Ev::Open { cursor, underlying, .. } => {
                live.retain(|(n, _)| n != cursor);
                live.push((cursor.clone(), underlying.clone()));
            }
            Ev::Alias { from, to } => {
                if let Some(u) = live.iter().find(|(n, _)| n == from).map(|(_, u)| u.clone()) {
                    live.retain(|(n, _)| n != to);
                    live.push((to.clone(), u));
                }
            }
            Ev::Ref { .. } | Ev::Rebind { .. } => {}
            Ev::ScopedOpen { underlying, span } => {
                scoped.push((underlying.clone(), *span));
            }
            Ev::ScopedClose => {
                scoped.pop();
            }
            Ev::UniqueCandidate { recv, name: callee, span } => {
                if std::env::var("DREAM_TRACE_BORROW").is_ok() {
                    eprintln!(
                        "[cand {i}] recv={recv} callee={callee} cls={:?} mode={:?} live={:?} scoped={:?}",
                        local_class.iter().rev().find(|(n, _)| n == recv).map(|(_, c)| c),
                        receiver_modes.get(&format!(
                            "{}::{callee}",
                            if recv == "this" {
                                owner.to_string()
                            } else if let Some(f) = recv.strip_prefix("this.") {
                                field_types.get(f).cloned().unwrap_or_default()
                            } else {
                                local_class
                                    .iter()
                                    .rev()
                                    .find(|(n, _)| n == recv)
                                    .map(|(_, c)| c.clone())
                                    .unwrap_or_default()
                            }
                        )),
                        live,
                        scoped,
                    );
                }
                // Resolve the callee's receiver mode.
                let cls = if recv == "this" {
                    Some(owner.to_string())
                } else if let Some(f) = recv.strip_prefix("this.") {
                    field_types.get(f).cloned()
                } else {
                    local_class
                        .iter()
                        .rev()
                        .find(|(n, _)| n == recv)
                        .map(|(_, c)| c.clone())
                };
                let Some(cls) = cls else { continue };
                let mode = receiver_modes
                    .get(&format!("{cls}::{callee}"))
                    .copied()
                    .unwrap_or(ReceiverMode::Borrow);
                if mode != ReceiverMode::Unique {
                    continue;
                }
                // Strong cursors on this underlying, still referenced later?
                let strong_hit = live
                    .iter()
                    .find(|(_, u)| u == recv)
                    .and_then(|(n, _)| last_ref.get(n))
                    .map(|&last| last > i)
                    .unwrap_or(false);
                let scoped_hit = scoped.iter().any(|(u, _)| u == recv);
                if std::env::var("DREAM_TRACE_BORROW").is_ok() {
                    eprintln!(
                        "[verdict] recv={recv} callee={callee} strong={strong_hit} scoped={scoped_hit} live={live:?} last_cur={:?}",
                        live
                            .iter()
                            .find(|(n, _)| n == "cur")
                            .and_then(|(n, _)| last_ref.get(n)),
                    );
                }
                if strong_hit || scoped_hit {
                    let opened = scoped
                        .iter()
                        .find(|(u, _)| u == recv)
                        .map(|(_, s)| *s)
                        .or_else(|| {
                            live.iter()
                                .rev()
                                .find(|(n, u)| {
                                    u == recv && last_ref.get(n).map(|&l| l > i).unwrap_or(false)
                                })
                                .and_then(|(n, _)| {
                                    events.iter().find_map(|ev| match ev {
                                        Ev::Open { cursor, span, .. } if cursor == n => {
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
    /// W6-B entry point. Runs on clean programs after receiver modes are known.
    pub(in crate::analyzer) fn check_borrow_collisions(
        &mut self,
        node: &'a ProgramNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let summaries = compute_view_summaries(node);
        let class_names: std::collections::HashSet<String> =
            node.structs.iter().map(|s| s.name.text.clone()).collect();

        for s in node.structs.iter() {
            let mut field_types: indexmap::IndexMap<String, String> = indexmap::IndexMap::new();
            let mut class_fields: Vec<String> = Vec::new();
            for f in s.fields.iter() {
                if let Some(owner) = type_owner_name(&f.field_type) {
                    field_types.insert(f.name.text.clone(), owner);
                }
                class_fields.push(f.name.text.clone());
            }

            for m in &s.methods {
                if m.is_static || m.is_extern || m.body.is_empty() {
                    continue;
                }
                let mut ex = Extractor {
                    summaries: &summaries,
                    class_names: &class_names,
                    _owner: s.name.text.clone(),
                    aliases_this: Vec::new(),
                    events: Vec::new(),
                    local_class: Vec::new(),
                };
                ex.walk_block(m.body, &field_types, &class_fields);

                interpret_events(
                    &ex.events,
                    &s.name.text,
                    &self.receiver_modes,
                    &field_types,
                    &ex.local_class,
                    &s.file_path,
                    diagnostics,
                );
            }
        }

        // Top-level free functions (including `main`): no receiver — only local-based borrows.
        for f in node.functions.iter() {
            if f.is_static || f.is_extern || f.body.is_empty() {
                continue;
            }
            let mut ex = Extractor {
                summaries: &summaries,
                class_names: &class_names,
                _owner: String::new(),
                aliases_this: Vec::new(),
                events: Vec::new(),
                local_class: Vec::new(),
            };
            ex.walk_block(f.body, &indexmap::IndexMap::new(), &[]);

            interpret_events(
                &ex.events,
                "",
                &self.receiver_modes,
                &indexmap::IndexMap::new(),
                &ex.local_class,
                &f.file_path,
                diagnostics,
            );
        }
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

impl<'s> Extractor<'s> {
    fn walk_block(
        &mut self,
        stmts: &[StatementNode],
        field_types: &indexmap::IndexMap<String, String>,
        class_fields: &[String],
    ) {
        for s in stmts {
            self.walk_stmt(s, field_types, class_fields);
        }
    }

    fn walk_args(
        &mut self,
        args: &[ExpressionNode],
        field_types: &indexmap::IndexMap<String, String>,
        class_fields: &[String],
    ) {
        for a in args {
            self.walk_expr(a, field_types, class_fields);
        }
    }

    fn emit_call_events(
        &mut self,
        receiver: &ExpressionNode,
        name: &str,
        args: &[ExpressionNode],
        field_types: &indexmap::IndexMap<String, String>,
        class_fields: &[String],
    ) {
        match canonical_recv(receiver, &self.aliases_this) {
            Some(recv_key) => {
                // Calling through a cursor keeps it alive — record the reference.
                self.events.push(Ev::Ref {
                    name: recv_key.clone(),
                });
                if let Some(span) = init_span(receiver) {
                    self.events.push(Ev::UniqueCandidate {
                        recv: recv_key,
                        name: name.to_string(),
                        span,
                    });
                }
            }
            // Chained receivers (`a.b(c).d(e)`): recurse so every nested call is seen.
            None => self.walk_expr(receiver, field_types, class_fields),
        }
        self.walk_args(args, field_types, class_fields);
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_stmt(
        &mut self,
        stmt: &StatementNode,
        field_types: &indexmap::IndexMap<String, String>,
        class_fields: &[String],
    ) {
        match stmt {
            StatementNode::Declaration(name_tok, _, init, _) => {
                if is_self_expr(init, &self.aliases_this) {
                    self.aliases_this.push(name_tok.text.clone());
                }
                // Constructor-call class inference: `let xs = List<int>();`,
                // `let w = Widget(3);` — callee identifier names a declared class.
                #[allow(clippy::collapsible_match)]
                let ctor_class: Option<&String> = match init {
                    ExpressionNode::FunctionCall(callee, _, _) => {
                        self.class_names.get(&callee.text)
                    }
                    ExpressionNode::Call(callee_expr, _, _) => {
                        match &**callee_expr {
                            ExpressionNode::Identifier(t) => self.class_names.get(&t.text),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                if let Some(class) = ctor_class {
                    self.local_class.retain(|(n, _)| n != &name_tok.text);
                    self.local_class
                        .push((name_tok.text.clone(), class.clone()));
                } else if std::env::var("DREAM_TRACE_BORROW").is_ok() {
                    eprintln!(
                        "[borrow] no ctor class for '{}' init={:?}",
                        name_tok.text,
                        init,
                    );
                }
                match init {
                    ExpressionNode::ArrayLiteral(..) => {
                        self.local_class.retain(|(n, _)| n != &name_tok.text);
                        self.local_class.push((name_tok.text.clone(), "__array".to_string()));
                    }
                    ExpressionNode::SetLiteral(..) => {
                        self.local_class.retain(|(n, _)| n != &name_tok.text);
                        self.local_class.push((name_tok.text.clone(), "Set".to_string()));
                    }
                    ExpressionNode::MapLiteral(..) => {
                        self.local_class.retain(|(n, _)| n != &name_tok.text);
                        self.local_class.push((name_tok.text.clone(), "Map".to_string()));
                    }
                    _ => {}
                }
                self.emit_binding_and_init(&name_tok.text, init, field_types, class_fields);
            }
            StatementNode::TupleDeclaration { init, .. } => {
                self.walk_expr(init, field_types, class_fields);
            }
            StatementNode::Assignment(name_tok, value) => {
                if class_fields.contains(&name_tok.text) {
                    if let Some(span) = Some(name_tok.position) {
                        self.events.push(Ev::UniqueCandidate {
                            recv: "this".to_string(),
                            name: format!("set {}", name_tok.text),
                            span,
                        });
                    }
                }
                self.emit_binding_and_init(&name_tok.text, value, field_types, class_fields);
            }
            StatementNode::MemberAssignment(target, name, value) => {
                if let Some(key) = canonical_recv(target, &self.aliases_this) {
                    if let Some(span) = Some(name.position) {
                        self.events.push(Ev::UniqueCandidate {
                            recv: key.clone(),
                            name: format!("set {}", name.text),
                            span,
                        });
                        if key.starts_with("this.") {
                            self.events.push(Ev::UniqueCandidate {
                                recv: "this".to_string(),
                                name: format!("set {}", name.text),
                                span,
                            });
                        }
                    }
                } else {
                    self.walk_expr(target, field_types, class_fields);
                }
                self.walk_expr(value, field_types, class_fields);
            }
            StatementNode::IndexAssignment(target, index, value) => {
                if let Some(key) = canonical_recv(target, &self.aliases_this) {
                    if let Some(span) = target_span_opt(target) {
                        self.events.push(Ev::UniqueCandidate {
                            recv: key.clone(),
                            name: "set_index".to_string(),
                            span,
                        });
                        if key.starts_with("this.") {
                            self.events.push(Ev::UniqueCandidate {
                                recv: "this".to_string(),
                                name: "set_index".to_string(),
                                span,
                            });
                        }
                    }
                } else {
                    self.walk_expr(target, field_types, class_fields);
                }
                self.walk_expr(index, field_types, class_fields);
                self.walk_expr(value, field_types, class_fields);
            }
            StatementNode::FunctionInvocation(callee, _, args) => {
                self.walk_args(args, field_types, class_fields);
                let _ = callee;
            }
            StatementNode::MethodInvocation(receiver, name, _, args) => {
                self.emit_call_events(receiver, &name.text, args, field_types, class_fields);
            }
            StatementNode::AwaitStmt(e) => self.walk_expr(e, field_types, class_fields),
            StatementNode::Return(Some(e)) => self.walk_expr(e, field_types, class_fields),
            StatementNode::IfElse(cond, then_b, elifs, else_b) => {
                self.walk_expr(cond, field_types, class_fields);
                self.walk_block(then_b, field_types, class_fields);
                for (c, b) in elifs {
                    self.walk_expr(c, field_types, class_fields);
                    self.walk_block(b, field_types, class_fields);
                }
                if let Some(b) = else_b {
                    self.walk_block(b, field_types, class_fields);
                }
            }
            StatementNode::While(cond, body) | StatementNode::DoWhile(body, cond) => {
                self.walk_expr(cond, field_types, class_fields);
                self.walk_block(body, field_types, class_fields);
            }
            StatementNode::For(init, cond, step, body) => {
                if let Some(s) = init {
                    self.walk_stmt(s, field_types, class_fields);
                }
                if let Some(c) = cond {
                    self.walk_expr(c, field_types, class_fields);
                }
                if let Some(s) = step {
                    self.walk_stmt(s, field_types, class_fields);
                }
                self.walk_block(body, field_types, class_fields);
            }
            StatementNode::Labeled(_, inner) => self.walk_stmt(inner, field_types, class_fields),
            StatementNode::ForEach(_, iterable, _, _, body) => {
                if let (Some(u), Some(span)) =
                    (canonical_recv(iterable, &self.aliases_this), init_span(iterable))
                {
                    self.events.push(Ev::ScopedOpen { underlying: u, span });
                    self.walk_block(body, field_types, class_fields);
                    self.events.push(Ev::ScopedClose);
                } else {
                    self.walk_expr(iterable, field_types, class_fields);
                    self.walk_block(body, field_types, class_fields);
                }
            }
            StatementNode::Switch(subject, arms, default_b) => {
                self.walk_expr(subject, field_types, class_fields);
                for (_, body) in arms {
                    self.walk_block(body, field_types, class_fields);
                }
                if let Some(b) = default_b {
                    self.walk_block(b, field_types, class_fields);
                }
            }
            StatementNode::Lock(target, body) => {
                self.walk_expr(target, field_types, class_fields);
                self.walk_block(body, field_types, class_fields);
            }
            StatementNode::ExpressionStatement(e) => self.walk_expr(e, field_types, class_fields),
            _ => {}
        }
    }

    fn walk_expr(
        &mut self,
        e: &ExpressionNode,
        field_types: &indexmap::IndexMap<String, String>,
        class_fields: &[String],
    ) {
        match e {
            ExpressionNode::Identifier(t) => {
                self.events.push(Ev::Ref {
                    name: t.text.clone(),
                });
            }
            ExpressionNode::Binary(lhs, _, rhs) => {
                self.walk_expr(lhs, field_types, class_fields);
                self.walk_expr(rhs, field_types, class_fields);
            }
            ExpressionNode::Ternary(cond, then_e, else_e) => {
                self.walk_expr(cond, field_types, class_fields);
                self.walk_expr(then_e, field_types, class_fields);
                self.walk_expr(else_e, field_types, class_fields);
            }
            ExpressionNode::Unary(_, inner)
            | ExpressionNode::Parenthesized(_, inner)
            | ExpressionNode::Try(inner) => {
                self.walk_expr(inner, field_types, class_fields)
            }
            ExpressionNode::IncDec { target, .. } => {
                if let Some(key) = canonical_recv(target, &self.aliases_this) {
                    if let Some(span) = target_span_opt(target) {
                        self.events.push(Ev::UniqueCandidate {
                            recv: key,
                            name: "incdec".to_string(),
                            span,
                        });
                    }
                }
                self.walk_expr(target, field_types, class_fields);
            }
            ExpressionNode::ArrayLiteral(_, elems)
            | ExpressionNode::TupleLiteral(_, elems)
            | ExpressionNode::SetLiteral(_, elems) => {
                self.walk_args(elems, field_types, class_fields);
            }
            ExpressionNode::MapLiteral(_, pairs) => {
                for (k, v) in pairs {
                    self.walk_expr(k, field_types, class_fields);
                    self.walk_expr(v, field_types, class_fields);
                }
            }
            ExpressionNode::FunctionCall(callee, _, args) => {
                self.walk_args(args, field_types, class_fields);
                let _ = callee;
            }
            ExpressionNode::Call(callee, _, args) => {
                self.walk_expr(callee, field_types, class_fields);
                self.walk_args(args, field_types, class_fields);
            }
            ExpressionNode::MethodCall(receiver, name, _, args) => {
                self.emit_call_events(receiver, &name.text, args, field_types, class_fields);
            }
            ExpressionNode::IndexAccess(base, index) => {
                self.walk_expr(base, field_types, class_fields);
                self.walk_expr(index, field_types, class_fields);
            }
            ExpressionNode::Cast(_, _, inner)
            | ExpressionNode::IsExpression(inner, _, _)
            | ExpressionNode::Await(_, inner) => {
                self.walk_expr(inner, field_types, class_fields)
            }
            ExpressionNode::MemberAccess(base, _) => {
                self.walk_expr(base, field_types, class_fields)
            }
            ExpressionNode::Switch(_, subject, arms) => {
                self.walk_expr(subject, field_types, class_fields);
                for arm in arms {
                    match &arm.body {
                        dream_syntax::nodes::expression::SwitchArmBody::Expr(expr) => {
                            self.walk_expr(expr, field_types, class_fields)
                        }
                        dream_syntax::nodes::expression::SwitchArmBody::Block(stmts) => {
                            self.walk_block(stmts, field_types, class_fields)
                        }
                    }
                }
            }
            ExpressionNode::Lambda(_) => {
                // Lifted bodies analyzed separately.
            }
            ExpressionNode::NamedArg(_, inner) | ExpressionNode::RefArgument(_, inner) => {
                self.walk_expr(inner, field_types, class_fields)
            }
            _ => {}
        }
    }
}

fn target_span_opt(t: &ExpressionNode) -> Option<TextSpan> {
    fn deepest(e: &ExpressionNode) -> Option<TextSpan> {
        match e {
            ExpressionNode::Identifier(t) => Some(t.position),
            ExpressionNode::MemberAccess(_, m) => Some(m.position),
            ExpressionNode::IndexAccess(b, _) => deepest(b),
            _ => None,
        }
    }
    deepest(t)
}
