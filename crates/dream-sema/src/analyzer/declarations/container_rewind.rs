//! Compile-time warning for the "rewind without release" ARC leak pattern in hand-written
//! containers: a class owns both a `T[]` buffer field of managed elements and an `int` count-like
//! field, and some method rewrites the count field without storing into the buffer (directly or
//! through any same-class method call). Rewinding such a counter leaves dropped elements strongly
//! referenced from dead slots until those slots happen to be overwritten — the bug class behind
//! `Queue::clear`/`PriorityQueue::clear` before they zeroed slots.
//!
//! The check is deliberately conservative to keep the false-positive rate near zero:
//! - classes without both a managed-element array field and a scalar int field are skipped;
//! - any indexed store into a buffer field anywhere in the method covers it;
//! - any call on `this` (or a bare sibling call) counts as covering, because the callee may zero
//!   slots — this lint does not build cross-method summaries;
//! - the `where T : unmanaged` clear idiom compiles to a separate specialization whose element
//!   type is not managed, so it never reaches this analysis.

use super::*;
use dream_syntax::nodes::expression::{LambdaBody, SwitchArmBody};
use dream_syntax::nodes::statement::StatementNode;
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::types::Type;

/// Fields of one candidate class: the managed-element array buffers and the scalar counters that
/// might be rewound over them.
struct ContainerFields {
    buffers: Vec<String>,
    counters: Vec<String>,
}

impl<'a> Analyzer<'a> {
    /// Warns when a method assigns a container's counter field with no reachable store into a
    /// paired buffer field. Called from [`Self::register_structs`] alongside the reference-cycle
    /// check.
    pub(in crate::analyzer) fn check_container_rewinds(
        &self,
        node: &'a ProgramNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for struct_decl in node.structs.iter() {
            if struct_decl.is_value || struct_decl.is_static {
                continue;
            }
            let Some(fields) = self.container_fields(struct_decl) else {
                continue;
            };
            let sibling_names: Vec<String> = struct_decl
                .methods
                .iter()
                .map(|m| m.name.text.clone())
                .collect();
            for method in &struct_decl.methods {
                // Constructors initialize counters on a fresh (empty) buffer — nothing to release.
                if method.is_static || method.is_extern || method.name.text == "constructor" {
                    continue;
                }
                let mut scan = MethodScan::default();
                scan_statements(method.body, &fields, &sibling_names, &mut scan);
                if !scan.count_writes.is_empty() && !scan.covered() {
                    diagnostics.file_path = file_path_string(&struct_decl.file_path);
                    diagnostics.report_warning(
                        format!(
                            "method '{}' of class '{}' writes field '{}' but stores no element into any buffer field ({}); if this shrinks the live element count, managed elements stay retained in dead slots until overwritten — zero the released slots first, or gate the method with 'where T : unmanaged'",
                            method.name.text,
                            struct_decl.name.text,
                            scan.count_writes[0],
                            fields.buffers.join(", "),
                        ),
                        Some(struct_decl.name.position),
                    );
                }
            }
        }
    }

    /// Classifies `(buffer field, counter field)` pairs for a class, or returns `None` when the
    /// declaration cannot exhibit the rewind pattern.
    fn container_fields(&self, decl: &StructDeclarationNode) -> Option<ContainerFields> {
        let mut buffers = Vec::new();
        let mut counters = Vec::new();
        for field in &decl.fields {
            match &field.field_type {
                Type::Array(inner) => {
                    if self.holds_managed(inner.as_ref()) {
                        buffers.push(field.name.text.clone());
                    }
                }
                Type::Integer(_) => counters.push(field.name.clone().text),
                _ => {}
            }
        }
        if buffers.is_empty() || counters.is_empty() {
            return None;
        }
        Some(ContainerFields { buffers, counters })
    }

    /// True when `ty` transitively contains an RC-tracked value: a class, `string`, `object`, or
    /// arrays of those. Primitives and user value structs are not managed.
    fn holds_managed(&self, ty: &Type) -> bool {
        match ty {
            Type::String(_) | Type::Object(_) => true,
            Type::Array(inner) => self.holds_managed(inner),
            Type::Struct(token, _) => self
                .struct_table
                .get_struct(&token.text)
                .map(|i| !i.is_value)
                .unwrap_or(false),
            _ => false,
        }
    }
}

/// Per-method scan state: which counter fields were written, whether any paired buffer was
/// indexed, and whether control flow reached another method of the class.
#[derive(Default)]
struct MethodScan {
    count_writes: Vec<String>,
    stored_buffers: Vec<String>,
    called_this: bool,
}

impl MethodScan {
    fn covered(&self) -> bool {
        !self.stored_buffers.is_empty() || self.called_this
    }
}

fn scan_statements(
    stmts: &[StatementNode],
    fields: &ContainerFields,
    siblings: &[String],
    scan: &mut MethodScan,
) {
    for stmt in stmts {
        scan_statement(stmt, fields, siblings, scan);
    }
}

fn scan_statement(
    stmt: &StatementNode,
    fields: &ContainerFields,
    siblings: &[String],
    scan: &mut MethodScan,
) {
    match stmt {
        StatementNode::Assignment(name_tok, value) => {
            note_field_write(&name_tok.text, fields, scan);
            scan_expression(value, fields, siblings, scan);
        }
        StatementNode::IndexAssignment(target, index, value) => {
            note_buffer_store(target, fields, scan);
            scan_expression(index, fields, siblings, scan);
            scan_expression(value, fields, siblings, scan);
        }
        StatementNode::MemberAssignment(target, name, value) => {
            if is_self_receiver(target) {
                note_field_write(&name.text, fields, scan);
            } else {
                // `obj.items[i] = v` through another expression still counts as covering.
                note_buffer_store(target, fields, scan);
                scan_expression(target, fields, siblings, scan);
            }
            scan_expression(value, fields, siblings, scan);
        }
        StatementNode::Declaration(_, _, init, _) => scan_expression(init, fields, siblings, scan),
        StatementNode::TupleDeclaration { init, .. } => {
            scan_expression(init, fields, siblings, scan)
        }
        StatementNode::FunctionInvocation(callee, _, args) => {
            if siblings.iter().any(|s| s == &callee.text) {
                scan.called_this = true;
            }
            for a in args {
                scan_expression(a, fields, siblings, scan);
            }
        }
        StatementNode::MethodInvocation(receiver, _, _, args) => {
            if is_self(receiver) {
                scan.called_this = true;
            }
            for a in args {
                scan_expression(a, fields, siblings, scan);
            }
        }
        StatementNode::Return(Some(e)) => scan_expression(e, fields, siblings, scan),
        StatementNode::IfElse(cond, then_b, elifs, else_b) => {
            scan_expression(cond, fields, siblings, scan);
            scan_statements(then_b, fields, siblings, scan);
            for (c, b) in elifs {
                scan_expression(c, fields, siblings, scan);
                scan_statements(b, fields, siblings, scan);
            }
            if let Some(b) = else_b {
                scan_statements(b, fields, siblings, scan);
            }
        }
        StatementNode::While(cond, body) | StatementNode::DoWhile(body, cond) => {
            scan_expression(cond, fields, siblings, scan);
            scan_statements(body, fields, siblings, scan);
        }
        StatementNode::For(init, cond, step, body) => {
            if let Some(s) = init {
                scan_statement(s, fields, siblings, scan);
            }
            if let Some(c) = cond {
                scan_expression(c, fields, siblings, scan);
            }
            if let Some(s) = step {
                scan_statement(s, fields, siblings, scan);
            }
            scan_statements(body, fields, siblings, scan);
        }
        StatementNode::Labeled(_, inner) => scan_statement(inner, fields, siblings, scan),
        StatementNode::ForEach(_, iterable, _, _, body) => {
            scan_expression(iterable, fields, siblings, scan);
            scan_statements(body, fields, siblings, scan);
        }
        StatementNode::Switch(subject, arms, default_b) => {
            scan_expression(subject, fields, siblings, scan);
            for (_, body) in arms {
                scan_statements(body, fields, siblings, scan);
            }
            if let Some(b) = default_b {
                scan_statements(b, fields, siblings, scan);
            }
        }
        StatementNode::Lock(target, body) => {
            scan_expression(target, fields, siblings, scan);
            scan_statements(body, fields, siblings, scan);
        }
        StatementNode::ExpressionStatement(e) | StatementNode::AwaitStmt(e) => {
            scan_expression(e, fields, siblings, scan);
        }
        _ => {}
    }
}

fn scan_expression(
    e: &ExpressionNode,
    fields: &ContainerFields,
    siblings: &[String],
    scan: &mut MethodScan,
) {
    match e {
        ExpressionNode::Binary(lhs, _, rhs) => {
            scan_expression(lhs, fields, siblings, scan);
            scan_expression(rhs, fields, siblings, scan);
        }
        ExpressionNode::Ternary(cond, then_e, else_e) => {
            scan_expression(cond, fields, siblings, scan);
            scan_expression(then_e, fields, siblings, scan);
            scan_expression(else_e, fields, siblings, scan);
        }
        ExpressionNode::Unary(_, inner)
        | ExpressionNode::Parenthesized(_, inner)
        | ExpressionNode::Try(inner) => scan_expression(inner, fields, siblings, scan),
        ExpressionNode::IncDec { target, .. } => {
            note_field_incdec(target, fields, scan);
            scan_expression(target, fields, siblings, scan);
        }
        ExpressionNode::ArrayLiteral(_, elems)
        | ExpressionNode::TupleLiteral(_, elems)
        | ExpressionNode::SetLiteral(_, elems) => {
            for x in elems {
                scan_expression(x, fields, siblings, scan);
            }
        }
        ExpressionNode::MapLiteral(_, pairs) => {
            for (k, v) in pairs {
                scan_expression(k, fields, siblings, scan);
                scan_expression(v, fields, siblings, scan);
            }
        }
        ExpressionNode::FunctionCall(callee, _, args) => {
            if siblings.iter().any(|s| s == &callee.text) {
                scan.called_this = true;
            }
            for a in args {
                scan_expression(a, fields, siblings, scan);
            }
        }
        ExpressionNode::Call(callee, _, args) => {
            scan_expression(callee, fields, siblings, scan);
            for a in args {
                scan_expression(a, fields, siblings, scan);
            }
        }
        ExpressionNode::MethodCall(receiver, _, _, args) => {
            if is_self(receiver) {
                scan.called_this = true;
            }
            for a in args {
                scan_expression(a, fields, siblings, scan);
            }
        }
        ExpressionNode::IndexAccess(base, index) => {
            note_buffer_store(base, fields, scan);
            scan_expression(index, fields, siblings, scan);
        }
        ExpressionNode::Cast(_, _, inner)
        | ExpressionNode::IsExpression(inner, _, _)
        | ExpressionNode::Await(_, inner) => scan_expression(inner, fields, siblings, scan),
        ExpressionNode::MemberAccess(base, _) => scan_expression(base, fields, siblings, scan),
        ExpressionNode::Switch(_, subject, arms) => {
            scan_expression(subject, fields, siblings, scan);
            for arm in arms {
                match &arm.body {
                    SwitchArmBody::Expr(e) => scan_expression(e, fields, siblings, scan),
                    SwitchArmBody::Block(stmts) => {
                        scan_statements(stmts, fields, siblings, scan)
                    }
                }
            }
        }
        ExpressionNode::Lambda(l) => {
            if let LambdaBody::Block(stmts) = &l.body {
                scan_statements(stmts, fields, siblings, scan);
            }
        }
        ExpressionNode::NamedArg(_, inner) | ExpressionNode::RefArgument(_, inner) => {
            scan_expression(inner, fields, siblings, scan)
        }
        _ => {}
    }
}

/// Records an indexed store when `base` names a buffer field, either directly (`items[i]`) or
/// through `this` (`this.items[i]`).
fn note_buffer_store(base: &ExpressionNode, fields: &ContainerFields, scan: &mut MethodScan) {
    let name = match base {
        ExpressionNode::Identifier(t) => Some(&t.text),
        ExpressionNode::MemberAccess(recv, member) => {
            matches!(*recv, ExpressionNode::Identifier(t) if t.text == "this")
                .then_some(&member.text)
        }
        _ => None,
    };
    if let Some(name) = name {
        if fields.buffers.iter().any(|b| b == name) {
            scan.stored_buffers.push(name.clone());
        }
    }
}

/// `this.count = …` / bare `count = …` both write the field inside a method body.
fn note_field_write(name: &str, fields: &ContainerFields, scan: &mut MethodScan) {
    if fields.counters.iter().any(|c| c == name) && !scan.count_writes.iter().any(|w| w == name) {
        scan.count_writes.push(name.to_string());
    }
}

/// `count++` / `--this.count`: the target is the field itself, not a member assignment.
fn note_field_incdec(
    target: &ExpressionNode,
    fields: &ContainerFields,
    scan: &mut MethodScan,
) {
    let name = match target {
        ExpressionNode::Identifier(t) => Some(&t.text),
        ExpressionNode::MemberAccess(recv, member) => {
            matches!(*recv, ExpressionNode::Identifier(t) if t.text == "this")
                .then_some(&member.text)
        }
        _ => None,
    };
    if let Some(name) = name {
        note_field_write(name, fields, scan);
    }
}

fn is_self(e: &ExpressionNode) -> bool {
    matches!(e, ExpressionNode::Identifier(t) if t.text == "this")
}

fn is_self_receiver(e: &ExpressionNode) -> bool {
    match e {
        ExpressionNode::Identifier(_) | ExpressionNode::MemberAccess(..) => {
            // Only `this.<field>` matters here; peel nothing else.
            matches!(e, ExpressionNode::Identifier(_))
                || matches!(e, ExpressionNode::MemberAccess(base, _) if matches!(*base, ExpressionNode::Identifier(t) if t.text == "this"))
        }
        _ => false,
    }
}
