//! Inferred receiver exclusivity (W6-A2): classifies every non-static method's implicit `this`
//! receiver as [`ReceiverMode::Borrow`] or [`ReceiverMode::Unique`].
//!
//! Modes are computed by a fixpoint over all method bodies in the program (classes, extend
//! blocks, interface default impls):
//!
//! - **direct mutation** — any write to a field of `this` (`this.count += 1`, `items[i] = v`,
//!   bare `count = 0` when `count` is a field) makes the method Unique;
//! - **field-chain mutation** — calling an already-Unique method through a field of `this`
//!   (`this.items.sort()`) mutates the instance's observable state and forces Unique too;
//! - **escaping `this`** — passing/storing `this` into another call may let the callee mutate
//!   it, so the caller becomes Unique conservatively;
//! - **sibling calls** propagate: a Borrow method cannot call a Unique sibling through shared
//!   `this` (the reentrant direction — Unique calling Borrow — stays legal).
//!
//! Explicit `[borrow | unique] fun` qualifiers pin the contract: pinned methods feed their
//! declared mode into the graph unchanged, and a declared `borrow` whose body resolves to
//! Unique is a dual-span error. Signature-only interface methods must declare a qualifier.
//!
//! The pass runs once, after body analysis, on clean programs. It reports diagnostics only —
//! the resolved modes land in `Analyzer::receiver_modes` for later consumers (dispatch
//! metadata, borrow-collision checking).

use super::*;
use dream_syntax::nodes::function::{FunctionNode, ReceiverMode};
use dream_syntax::nodes::statement::StatementNode;
use dream_text::text_span::TextSpan;

/// Registry key for one method: `"Owner::name"` where Owner is a class, an extended type's
/// spelling (`int`, `string`, ...), or an interface name.
type MethodKey = String;

#[derive(Debug, Clone, PartialEq, Eq)]
enum RecvKind {
    /// Bare `this` receiver (or an alias of it): the call targets a sibling.
    This,
    /// One-level field chain: `this.<field>.<method>(...)`.
    Field(String),
}

/// One registry entry: everything known about a single method.
struct Entry {
    owner: String,
    name: String,
    /// Explicit `[borrow | unique]` qualifier; pins the mode.
    explicit: Option<ReceiverMode>,
    decl_span: TextSpan,
    /// Body writes a field of `this`.
    direct_unique: bool,
    first_mutate_span: Option<TextSpan>,
    /// Unresolved calls: `(receiver kind, callee name, call span)`. Resolved to registry keys
    /// after all entries exist.
    raw_calls: Vec<(RecvKind, String, TextSpan)>,
    /// Set by the fixpoint when inference concludes Unique (explicitly-pinned `unique` methods
    /// are already unique via `effective_mode`; pinned `borrow` methods never get marked here,
    /// but their *body facts* still trigger the contradiction diagnostic below).
    inferred_unique: bool,
    /// True for signature-only interface methods (no body to infer from; explicit mode required).
    is_interface_signature: bool,
}

impl Entry {
    fn effective_mode(&self) -> ReceiverMode {
        match self.explicit {
            Some(m) => m,
            None => {
                // NOTE: handing `this` to another call (takes_this) deliberately does NOT
                // force Unique — sharing a reference grants no mutation rights under ARC, and
                // treating it as mutation mis-flagged `List.iterator()` (which retains the list
                // inside its cursor) against Borrow-declaring interfaces.
                if self.inferred_unique || self.direct_unique {
                    ReceiverMode::Unique
                } else {
                    ReceiverMode::Borrow
                }
            }
        }
    }

    fn is_unique(&self) -> bool {
        self.effective_mode() == ReceiverMode::Unique
    }
}

fn new_entry(owner: String, method: &FunctionNode) -> Entry {
    Entry {
        owner,
        name: method.name.text.clone(),
        explicit: method.receiver_mode,
        decl_span: method.name.position,
        direct_unique: false,
        first_mutate_span: None,
        raw_calls: Vec::new(),
        inferred_unique: false,
        is_interface_signature: false,
    }
}

/// The owner name a field's declared type routes method calls to: classes, primitives covered
/// by extend blocks (`string`, `int`, ...), interfaces — anything whose methods live in the
/// registry under `Owner::name`.
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
        Type::ISize(_) => Some("isize".to_string()),
        Type::USize(_) => Some("usize".to_string()),
        _ => None,
    }
}

impl<'a> Analyzer<'a> {
    /// Classifies every method's receiver mode. Called from `analyze_pgm` after body analysis,
    /// on programs with no other errors (a poisoned program skips straight to failure).
    pub(in crate::analyzer) fn classify_receiver_modes(
        &mut self,
        node: &'a ProgramNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let mut registry: indexmap::IndexMap<MethodKey, Entry> = indexmap::IndexMap::new();

        // --- Collect entries + raw facts ----------------------------------------------------
        for struct_decl in node.structs.iter() {
            let owner = struct_decl.name.text.clone();
            let field_names: Vec<String> = struct_decl
                .fields
                .iter()
                .map(|f| f.name.text.clone())
                .collect();
            for method in &struct_decl.methods {
                if method.is_static {
                    continue;
                }
                let key = format!("{owner}::{}", method.name.text);
                let mut entry = new_entry(owner.clone(), method);
                walk_body_for_facts(
                    method.body,
                    &field_names,
                    &mut entry.direct_unique,
                    &mut entry.first_mutate_span,
                    &mut entry.raw_calls,
                );
                registry.insert(key, entry);
            }
        }

        for ext in node.extends.iter() {
            let owner = ext.target.text.clone();
            // Value-struct extend targets have registered fields; primitive targets have none
            // (their methods classify as Borrow unless they take/escape `this`, which they
            // cannot — primitives have no `this` state to escape).
            let field_names: Vec<String> = match self.struct_table.get_struct(&owner) {
                Some(info) => info.fields.keys().cloned().collect(),
                None => Vec::new(),
            };
            for method in &ext.methods {
                if method.is_static {
                    continue;
                }
                let key = format!("{owner}::{}", method.name.text);
                let mut entry = new_entry(owner.clone(), method);
                walk_body_for_facts(
                    method.body,
                    &field_names,
                    &mut entry.direct_unique,
                    &mut entry.first_mutate_span,
                    &mut entry.raw_calls,
                );
                registry.insert(key, entry);
            }
        }

        for iface in node.interfaces.iter() {
            let owner = iface.name.text.clone();
            for method in &iface.methods {
                if method.is_static {
                    continue;
                }
                let key = format!("{owner}::{}", method.name.text);
                let has_body = !method.body.is_empty();
                let mut entry = new_entry(owner.clone(), method);
                entry.is_interface_signature = !has_body;
                // Signature-only methods default to Borrow — the overwhelmingly common
                // contract. Mutating contracts opt in with `unique` (e.g. `Iterator.next`
                // advancing its cursor). If an implementor's body turns out Unique, the
                // conformance mismatch surfaces there instead of breaking the interface.
                if has_body {
                    walk_body_for_facts(
                        method.body,
                        &[],
                        &mut entry.direct_unique,
                        &mut entry.first_mutate_span,
                        &mut entry.raw_calls,
                    );
                }
                registry.insert(key, entry);
            }
        }

        // --- Resolve raw calls to registry keys ---------------------------------------------
        // `This` calls resolve against the entry's owner; `Field(f)` calls resolve against the
        // field's declared owner type (class / extended primitive / interface). Unresolvable
        // calls (generics, unknown types) contribute no edge — conservative toward Borrow.
        let resolved_edges: indexmap::IndexMap<MethodKey, Vec<(MethodKey, TextSpan)>> = {
            let mut out: indexmap::IndexMap<MethodKey, Vec<(MethodKey, TextSpan)>> =
                indexmap::IndexMap::new();
            for (key, e) in &registry {
                let mut edges = Vec::new();
                for (recv, name, span) in &e.raw_calls {
                    let target_owner: Option<String> = match recv {
                        RecvKind::This => Some(e.owner.clone()),
                        RecvKind::Field(f) => match self.struct_table.get_struct(&e.owner) {
                            Some(info) => {
                                info.fields.get(f).and_then(|fi| type_owner_name(&fi.type_))
                            }
                            None => None,
                        },
                    };
                    if let Some(owner) = target_owner {
                        let k = format!("{owner}::{name}");
                        if registry.contains_key(&k) {
                            edges.push((k, *span));
                        }
                    }
                }
                out.insert(key.clone(), edges);
            }
            out
        };

        // --- Fixpoint -----------------------------------------------------------------------
        loop {
            let mut changed = false;
            let keys: Vec<MethodKey> = registry.keys().cloned().collect();
            for key in &keys {
                let should_mark = {
                    let e = &registry[key];
                    if e.explicit.is_some() || e.inferred_unique {
                        false
                    } else {
                        e.direct_unique
                            || resolved_edges[key]
                                .iter()
                                .any(|(t, _)| registry[t].is_unique())
                    }
                };
                if should_mark {
                    registry[key].inferred_unique = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        self.check_receiver_contracts(node, diagnostics, &registry, &resolved_edges);
    }
}

fn registry_lookup(
    modes: &HashMap<String, dream_syntax::nodes::function::ReceiverMode>,
    key: &str,
) -> dream_syntax::nodes::function::ReceiverMode {
    modes
        .get(key)
        .copied()
        .unwrap_or(dream_syntax::nodes::function::ReceiverMode::Borrow)
}

fn file_for_owner<'a>(node: &'a ProgramNode<'a>, owner: &str) -> Option<Rc<str>> {
    for s in node.structs.iter() {
        if s.name.text == owner {
            return s.file_path.clone();
        }
    }
    for e in node.extends.iter() {
        if e.target.text == owner {
            return e.methods.first().and_then(|m| m.file_path.clone());
        }
    }
    for i in node.interfaces.iter() {
        if i.name.text == owner {
            return i.file_path.clone();
        }
    }
    None
}

/// True when `expr` reads `this` directly or through a tracked alias.
mod walking;
use walking::walk_body_for_facts;

mod checking;
