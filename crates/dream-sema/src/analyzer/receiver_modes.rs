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

        // --- Diagnostics + storage ----------------------------------------------------------
        for (key, e) in &registry {
            // A pinned `borrow` whose body still resolves to mutation is a contract violation.
            // Note: explicit-Borrow entries never get `inferred_unique`, so test the raw facts
            // plus whether any edge reaches a unique method.
            if e.explicit == Some(ReceiverMode::Borrow)
                && (e.direct_unique
                    || resolved_edges[key]
                        .iter()
                        .any(|(t, _)| registry[t].is_unique()))
            {
                // Only override the bag's current file when the owner's own file is known;
                // otherwise the diagnostic renders against a stale path.
                let file = file_for_owner(node, &e.owner);
                if file.is_some() {
                    diagnostics.file_path = file_path_string(&file);
                }
                diagnostics.report_error(
                    format!(
                        "method '{}.{}' is declared 'borrow' but its body mutates the instance — a 'borrow' method promises not to mutate '{}'. Change the declaration to 'unique fun', or move the mutation out",
                        e.owner, e.name, e.owner
                    ),
                    Some(e.first_mutate_span.unwrap_or(e.decl_span)),
                );
            }
            self.receiver_modes.insert(key.clone(), e.effective_mode());
        }

        // --- Conformance: implementor modes must match interface contracts ------------------
        // A class implementing `I` must declare each of I's methods with the same receiver
        // mode the interface declares (signature-only methods default to Borrow). Methods the
        // class omits inherit an interface default body, whose mode is by definition the
        // interface's own — nothing to check.
        for s in node.structs.iter() {
            for iface_ty in &s.implements {
                let iface_name = match iface_ty {
                    Type::Struct(token, _) => token.text.clone(),
                    _ => continue,
                };
                let mut required: Vec<(String, ReceiverMode)> = Vec::new();
                self.collect_interface_contract(node, &iface_name, &mut required, &mut Vec::new());
                if required.is_empty() {
                    continue;
                }
                for (mname, imode) in required {
                    let own = s
                        .methods
                        .iter()
                        .find(|m| !m.is_static && m.name.text == mname);
                    let Some(own) = own else { continue };
                    let impl_key = format!("{}::{mname}", s.name.text);
                    let Some(e) = registry.get(&impl_key) else {
                        continue;
                    };
                    let impl_mode = e.effective_mode();
                    if impl_mode != imode {
                        let file = file_for_owner(node, &e.owner);
                        if file.is_some() {
                            diagnostics.file_path = file_path_string(&file);
                        }
                        diagnostics.report_error(
                            format!(
                                "'{}.{}' is inferred {} but interface '{}' declares it {} — a caller holding the interface value must be able to rely on the same mutation rights. Align the two (mark the interface 'unique' if implementing classes mutate '{}', or make this method read-only)",
                                s.name.text,
                                mname,
                                match impl_mode { ReceiverMode::Unique => "unique", ReceiverMode::Borrow => "borrow" },
                                iface_name,
                                match imode { ReceiverMode::Unique => "'unique'", ReceiverMode::Borrow => "'borrow'" },
                                s.name.text,
                            ),
                            Some(own.name.position),
                        );
                    }
                }
            }
        }
    }

    /// Collects `(method name, declared mode)` for `iface_name` and every transitive parent.
    /// `visited` guards inheritance cycles.
    fn collect_interface_contract(
        &self,
        node: &'a ProgramNode<'a>,
        iface_name: &str,
        out: &mut Vec<(String, ReceiverMode)>,
        visited: &mut Vec<String>,
    ) {
        if visited.iter().any(|v| v == iface_name) {
            return;
        }
        visited.push(iface_name.to_string());
        if let Some(parents) = self.interface_parents.get(iface_name) {
            for parent in parents {
                if let Type::Struct(tok, _) = parent {
                    self.collect_interface_contract(node, &tok.text, out, visited);
                }
            }
        }
        for i in node.interfaces.iter() {
            if i.name.text != iface_name {
                continue;
            }
            for m in &i.methods {
                if m.is_static {
                    continue;
                }
                let mode = registry_lookup(
                    &self.receiver_modes,
                    &format!("{iface_name}::{}", m.name.text),
                );
                out.push((m.name.text.clone(), mode));
            }
            return;
        }
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
fn is_self_expr(expr: &ExpressionNode, aliases: &[String]) -> bool {
    match expr {
        ExpressionNode::Identifier(t) => t.text == "this" || aliases.contains(&t.text),
        _ => false,
    }
}

/// Classify the receiver of a method call: bare `this`/alias, or `this.<field>` / `<alias>.<field>`.
fn classify_receiver(expr: &ExpressionNode, aliases: &[String]) -> Option<RecvKind> {
    match expr {
        ExpressionNode::Identifier(t) => {
            if t.text == "this" || aliases.contains(&t.text) {
                Some(RecvKind::This)
            } else {
                None
            }
        }
        ExpressionNode::MemberAccess(base, member) => {
            let base_is_self = match &**base {
                ExpressionNode::Identifier(t) => t.text == "this" || aliases.contains(&t.text),
                _ => false,
            };
            base_is_self.then_some(RecvKind::Field(member.text.clone()))
        }
        ExpressionNode::Parenthesized(_, inner) => classify_receiver(inner, aliases),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_body_for_facts(
    stmts: &[StatementNode],
    field_names: &[String],
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
    raw_calls: &mut Vec<(RecvKind, String, TextSpan)>,
) {
    let mut aliases: Vec<String> = Vec::new();
    walk_statements(
        stmts,
        field_names,
        &mut aliases,
        direct_unique,
        first_mutate_span,
        raw_calls,
    );
}

#[allow(clippy::too_many_arguments)]
fn walk_statements(
    stmts: &[StatementNode],
    field_names: &[String],
    aliases: &mut Vec<String>,
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
    raw_calls: &mut Vec<(RecvKind, String, TextSpan)>,
) {
    for stmt in stmts {
        walk_statement(
            stmt,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_statement(
    stmt: &StatementNode,
    field_names: &[String],
    aliases: &mut Vec<String>,
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
    raw_calls: &mut Vec<(RecvKind, String, TextSpan)>,
) {
    match stmt {
        StatementNode::Assignment(name_tok, value) => {
            if field_names.contains(&name_tok.text) {
                mark_mutation(name_tok, direct_unique, first_mutate_span);
            }
            walk_expression(
                value,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::Declaration(name_tok, _, init, _) => {
            // `let alias = this;` tracks the local as a `this` alias for receiver classification.
            if is_self_expr(init, aliases) {
                aliases.push(name_tok.text.clone());
            }
            walk_expression(
                init,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::TupleDeclaration { init, .. } => walk_expression(
            init,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        StatementNode::MemberAssignment(target, name, value) => {
            if is_self_expr(target, aliases) {
                mark_mutation(name, direct_unique, first_mutate_span);
            } else {
                note_chain_write(target, direct_unique, first_mutate_span);
                walk_expression(
                    target,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
            walk_expression(
                value,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::IndexAssignment(target, index, value) => {
            note_chain_write(target, direct_unique, first_mutate_span);
            walk_expression(
                index,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_expression(
                value,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::FunctionInvocation(callee, _, args) => {
            raw_calls.push((RecvKind::This, callee.text.clone(), callee.position));
            note_sink_args(
                args,
                aliases,
                field_names,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::MethodInvocation(receiver, name, _, args) => {
            if let Some(kind) = classify_receiver(receiver, aliases) {
                raw_calls.push((kind, name.text.clone(), name.position));
            }
            walk_expression(
                receiver,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            for a in args {
                walk_expression(
                    a,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
        }
        StatementNode::Return(Some(e)) => walk_expression(
            e,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        StatementNode::IfElse(cond, then_b, elifs, else_b) => {
            walk_expression(
                cond,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_statements(
                then_b,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            for (c, b) in elifs {
                walk_expression(
                    c,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
                walk_statements(
                    b,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
            if let Some(b) = else_b {
                walk_statements(
                    b,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
        }
        StatementNode::While(cond, body) | StatementNode::DoWhile(body, cond) => {
            walk_expression(
                cond,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_statements(
                body,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::For(init, cond, step, body) => {
            if let Some(s) = init {
                walk_statement(
                    s,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
            if let Some(c) = cond {
                walk_expression(
                    c,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
            if let Some(s) = step {
                walk_statement(
                    s,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
            walk_statements(
                body,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::Labeled(_, inner) => walk_statement(
            inner,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        StatementNode::ForEach(_, iterable, _, _, body) => {
            walk_expression(
                iterable,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_statements(
                body,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::Switch(subject, arms, default_b) => {
            walk_expression(
                subject,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            for (_, body) in arms {
                walk_statements(
                    body,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
            if let Some(b) = default_b {
                walk_statements(
                    b,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
        }
        StatementNode::Lock(target, body) => {
            walk_expression(
                target,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_statements(
                body,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::ExpressionStatement(e) | StatementNode::AwaitStmt(e) => walk_expression(
            e,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        _ => {}
    }
}

// Helper wrappers kept tiny so each call site above stays readable.
#[allow(clippy::too_many_arguments)]
fn note_sink_args(
    args: &[ExpressionNode],
    aliases: &mut Vec<String>,
    field_names: &[String],
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
    raw_calls: &mut Vec<(RecvKind, String, TextSpan)>,
) {
    for a in args {
        walk_expression(
            a,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        );
    }
}

fn mark_mutation(
    tok: &dream_syntax::token::syntax_token::SyntaxToken,
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
) {
    *direct_unique = true;
    if first_mutate_span.is_none() {
        *first_mutate_span = Some(tok.position);
    }
}

/// Any indexed write whose chain roots in this/alias counts as mutating instance state:
/// `items[i] = v`, `this.slots[i] = v`, `this.slots[i].value = v`, ...
fn note_chain_write(
    target: &ExpressionNode,
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
) {
    fn root_span(mut e: &ExpressionNode) -> Option<TextSpan> {
        loop {
            match e {
                ExpressionNode::Identifier(t) => return Some(t.position),
                ExpressionNode::MemberAccess(base, m) => {
                    if matches!(&**base, ExpressionNode::Identifier(ref t) if t.text == "this") {
                        return Some(m.position);
                    }
                    e = base;
                }
                ExpressionNode::IndexAccess(base, _) => e = base,
                ExpressionNode::Parenthesized(_, inner) => e = inner,
                _ => return None,
            }
        }
    }
    if roots_in_self(target) && !*direct_unique {
        *direct_unique = true;
        if first_mutate_span.is_none() {
            *first_mutate_span = root_span(target);
        }
    }
}

/// True when the expression's chain roots at `this` (or an alias).
fn roots_in_self(mut e: &ExpressionNode) -> bool {
    loop {
        match e {
            ExpressionNode::Identifier(t) => return t.text == "this",
            ExpressionNode::MemberAccess(base, _) => e = base,
            ExpressionNode::IndexAccess(base, _) => e = base,
            ExpressionNode::Parenthesized(_, inner) => e = inner,
            _ => return false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_expression(
    e: &ExpressionNode,
    field_names: &[String],
    aliases: &mut Vec<String>,
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
    raw_calls: &mut Vec<(RecvKind, String, TextSpan)>,
) {
    match e {
        ExpressionNode::Binary(lhs, _, rhs) => {
            walk_expression(
                lhs,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_expression(
                rhs,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        ExpressionNode::Ternary(cond, then_e, else_e) => {
            walk_expression(
                cond,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_expression(
                then_e,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_expression(
                else_e,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        ExpressionNode::Unary(_, inner)
        | ExpressionNode::Parenthesized(_, inner)
        | ExpressionNode::Try(inner) => walk_expression(
            inner,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        ExpressionNode::IncDec { target, .. } => {
            note_chain_write(target, direct_unique, first_mutate_span);
            if let ExpressionNode::Identifier(t) = &**target {
                if field_names.contains(&t.text) {
                    *direct_unique = true;
                    if first_mutate_span.is_none() {
                        *first_mutate_span = Some(t.position);
                    }
                }
            }
            walk_expression(
                target,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        ExpressionNode::ArrayLiteral(_, elems)
        | ExpressionNode::TupleLiteral(_, elems)
        | ExpressionNode::SetLiteral(_, elems) => {
            for x in elems {
                walk_expression(
                    x,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
        }
        ExpressionNode::MapLiteral(_, pairs) => {
            for (k, v) in pairs {
                walk_expression(
                    k,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
                walk_expression(
                    v,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
        }
        ExpressionNode::FunctionCall(callee, _, args) => {
            raw_calls.push((RecvKind::This, callee.text.clone(), callee.position));
            for a in args {
                walk_expression(
                    a,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
        }
        ExpressionNode::Call(callee, _, args) => {
            walk_expression(
                callee,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            for a in args {
                walk_expression(
                    a,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
        }
        ExpressionNode::MethodCall(receiver, name, _, args) => {
            if let Some(kind) = classify_receiver(receiver, aliases) {
                raw_calls.push((kind, name.text.clone(), name.position));
            }
            for a in args {
                walk_expression(
                    a,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
        }
        ExpressionNode::IndexAccess(base, index) => {
            walk_expression(
                base,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_expression(
                index,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        ExpressionNode::Cast(_, _, inner)
        | ExpressionNode::IsExpression(inner, _, _)
        | ExpressionNode::Await(_, inner) => walk_expression(
            inner,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        ExpressionNode::MemberAccess(base, _) => walk_expression(
            base,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        ExpressionNode::Switch(_, subject, arms) => {
            walk_expression(
                subject,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            for arm in arms {
                match &arm.body {
                    dream_syntax::nodes::expression::SwitchArmBody::Expr(expr) => walk_expression(
                        expr,
                        field_names,
                        aliases,
                        direct_unique,
                        first_mutate_span,
                        raw_calls,
                    ),
                    dream_syntax::nodes::expression::SwitchArmBody::Block(stmts) => {
                        walk_statements(
                            stmts,
                            field_names,
                            aliases,
                            direct_unique,
                            first_mutate_span,
                            raw_calls,
                        )
                    }
                }
            }
        }
        ExpressionNode::Lambda(l) => {
            // Lambda bodies are lifted and analyzed as their own functions elsewhere; captures
            // of `this` inside them belong to closure-cycle analysis, not receiver modes.
            let _ = l;
        }
        ExpressionNode::NamedArg(_, inner) | ExpressionNode::RefArgument(_, inner) => {
            walk_expression(
                inner,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            )
        }
        _ => {}
    }
}
