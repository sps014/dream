//! Compile-time error for the "rewind without release" ARC leak pattern in hand-written
//! containers: a class owns both a `T[]` buffer field of managed elements and an `int` count-like
//! field, and some method shrinks the counter field without releasing buffer slots (directly or
//! through any call). Rewinding such a counter leaves dropped elements strongly referenced from
//! dead slots until those slots happen to be overwritten — the bug class behind
//! `Queue::clear`/`PriorityQueue::clear` before they zeroed slots. The check is compile-time
//! only, so release builds carry zero runtime cost.
//!
//! Escapes, in order of preference:
//! - call `Buffer.clear(items)` / `Buffer.truncate(items, n)` (the error suggests these);
//! - declare the method or class parameter `where T : unmanaged` — nothing to release;
//! - pass the buffer to any call as an argument (delegated cleanup);
//! - annotate the method `@allow_rewind` to acknowledge the retention explicitly.

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
        // Value structs are unmanaged as elements, but their fields may hold references
        // (`struct Cell { label: string, owner: Boxed }`): shrinking an array of them must
        // release those fields. Precompute which value structs are managed-by-content.
        let mut ref_values: std::collections::HashSet<String> = std::collections::HashSet::new();
        self.collect_managed_value_structs(node, &mut ref_values);

        for struct_decl in node.structs.iter() {
            if struct_decl.is_value || struct_decl.is_static {
                continue;
            }
            let Some(fields) = self.container_fields(struct_decl, &ref_values) else {
                continue;
            };
            // A class-level `T : unmanaged` bound means no element can hold a reference.
            if class_is_unmanaged(struct_decl) {
                continue;
            }
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
                // `where T : unmanaged` specializations rewind legitimately; `@allow_rewind`
                // acknowledges retention explicitly.
                if method_has_unmanaged_bound(method)
                    || method
                        .attributes
                        .iter()
                        .any(|a| a.name.text == "allow_rewind")
                {
                    continue;
                }
                let mut scan = MethodScan::default();
                scan_statements(method.body, &fields, &sibling_names, &mut scan);
                if !scan.count_writes.is_empty() && !scan.covered() {
                    diagnostics.file_path = file_path_string(&struct_decl.file_path);
                    diagnostics.report_error(
                        format!(
                            "'{}.{}' shrinks '{}' without releasing '{}' — dropped elements stay strongly referenced from dead slots until those slots are overwritten. Call `Buffer.clear({})` or `Buffer.truncate({}, n)` before the counter update, pass the buffer to a helper that clears it, or mark the method `where T : unmanaged` / `@allow_rewind` if retention is intentional.",
                            struct_decl.name.text,
                            method.name.text,
                            scan.count_writes.join("', '"),
                            fields.buffers.join("', '"),
                            fields.buffers.first().cloned().unwrap_or_default(),
                            fields.buffers.first().cloned().unwrap_or_default(),
                        ),
                        Some(struct_decl.name.position),
                    );
                }
            }
        }
    }

    /// Classifies `(buffer field, counter field)` pairs for a class, or returns `None` when the
    /// declaration cannot exhibit the rewind pattern.
    fn container_fields(
        &self,
        decl: &StructDeclarationNode,
        ref_values: &std::collections::HashSet<String>,
    ) -> Option<ContainerFields> {
        let mut buffers = Vec::new();
        let mut counters = Vec::new();
        for field in &decl.fields {
            match &field.field_type {
                Type::Array(inner) => {
                    if self.holds_managed(inner.as_ref(), ref_values) {
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

    /// Value structs whose fields transitively hold an RC-tracked value. Fixed point so nested
    /// value structs (`struct Outer { inner: Cell }`) resolve without looping on cycles.
    fn collect_managed_value_structs(
        &self,
        node: &'a ProgramNode<'a>,
        out: &mut std::collections::HashSet<String>,
    ) {
        let value_decls: Vec<&StructDeclarationNode> =
            node.structs.iter().filter(|s| s.is_value).collect();
        loop {
            let mut grew = false;
            for decl in &value_decls {
                let name = decl.name.text.clone();
                if out.contains(&name) {
                    continue;
                }
                let holds = decl.fields.iter().any(|f| match &f.field_type {
                    Type::String(_) | Type::Object(_) | Type::Generic(_) => true,
                    Type::Array(inner) => self.holds_managed_shallow(inner.as_ref(), out),
                    Type::Struct(tok, _) => {
                        match self.struct_table.get_struct(&tok.text) {
                            // Another value struct: resolved by a later fixed-point pass.
                            Some(i) if i.is_value => out.contains(&tok.text),
                            // A class field makes the value struct managed-by-content.
                            Some(_) => true,
                            None => false,
                        }
                    }
                    _ => false,
                });
                if holds {
                    out.insert(name);
                    grew = true;
                }
            }
            if !grew {
                return;
            }
        }
    }

    fn holds_managed_shallow(
        &self,
        ty: &Type,
        ref_values: &std::collections::HashSet<String>,
    ) -> bool {
        match ty {
            Type::Struct(tok, _) => ref_values.contains(&tok.text),
            Type::Array(inner) => self.holds_managed_shallow(inner.as_ref(), ref_values),
            other => self.holds_managed(other, ref_values),
        }
    }

    /// True when `ty` transitively contains an RC-tracked value: a class, `string`, `object`,
    /// arrays of those, a value struct that contains references, or an unresolved generic
    /// parameter (conservative — `Queue<T>`'s `items: T[]` must count as managed even though
    /// `T` is only known at monomorphization). Primitives and reference-free value structs
    /// are not managed.
    fn holds_managed(&self, ty: &Type, ref_values: &std::collections::HashSet<String>) -> bool {
        match ty {
            Type::String(_) | Type::Object(_) | Type::Generic(_) => true,
            Type::Array(inner) => self.holds_managed(inner.as_ref(), ref_values),
            Type::Struct(token, args) => {
                if matches!(token.text.as_str(), "List" | "Set" | "Map" | "Option") {
                    if let Some(parts) = args.as_ref() {
                        return parts.iter().any(|a| self.holds_managed(a, ref_values));
                    }
                }
                match self.struct_table.get_struct(&token.text) {
                    Some(info) if info.is_value => ref_values.contains(&token.text),
                    Some(_) => true,
                    // Unknown named types are treated as managed to stay conservative.
                    None => true,
                }
            }
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
    /// Local names aliased to a buffer field (`let buf = this.items;`), so stores through the
    /// alias count as covering stores.
    buffer_locals: Vec<String>,
    called_this: bool,
}

impl MethodScan {
    /// True when `name` is a known alias for one of the class's buffer fields.
    fn is_buffer_name(&self, fields: &ContainerFields, name: &str) -> bool {
        fields.buffers.iter().any(|b| b == name)
            || self.buffer_locals.iter().any(|l| l == name)
    }
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
            note_counter_write(&name_tok.text, Some(value), fields, scan);
            note_buffer_reassign(&name_tok.text, fields, scan);
            scan_expression(value, fields, siblings, scan);
        }
        StatementNode::IndexAssignment(target, index, value) => {
            note_buffer_store(target, fields, scan);
            scan_expression(index, fields, siblings, scan);
            scan_expression(value, fields, siblings, scan);
        }
        StatementNode::MemberAssignment(target, name, value) => {
            if is_self_receiver(target) {
                note_counter_write(&name.text, Some(value), fields, scan);
                note_buffer_reassign(&name.text, fields, scan);
            } else {
                // `obj.items[i] = v` through another expression still counts as covering.
                note_buffer_store(target, fields, scan);
                scan_expression(target, fields, siblings, scan);
            }
            scan_expression(value, fields, siblings, scan);
        }
        StatementNode::Declaration(name_tok, _, init, _) => {
            // `let buf = this.items;` aliases a buffer field: subsequent `buf[i] = v` stores
            // release slots exactly like direct field stores.
            if let ExpressionNode::MemberAccess(recv, member) = init {
                if matches!(*recv, ExpressionNode::Identifier(ref t) if t.text == "this")
                    && fields.buffers.iter().any(|b| b == &member.text)
                {
                    scan.buffer_locals.push(name_tok.text.clone());
                }
            }
            scan_expression(init, fields, siblings, scan);
        }
        StatementNode::TupleDeclaration { init, .. } => {
            scan_expression(init, fields, siblings, scan)
        }
        StatementNode::FunctionInvocation(callee, _, args) => {
            if siblings.iter().any(|s| s == &callee.text) {
                scan.called_this = true;
            }
            for a in args {
                if let Some(t) = buffer_field_arg(a, fields, scan) {
                    scan.stored_buffers.push(t);
                }
                scan_expression(a, fields, siblings, scan);
            }
        }
        StatementNode::MethodInvocation(receiver, _, _, args) => {
            if is_self(receiver) {
                scan.called_this = true;
            }
            for a in args {
                if let Some(t) = buffer_field_arg(a, fields, scan) {
                    scan.stored_buffers.push(t);
                }
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
        ExpressionNode::IncDec {
            is_inc, target, ..
        } => {
            // Only decrements can rewind a counter; growth is harmless.
            if !is_inc {
                note_field_incdec(target, fields, scan);
            }
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
            // Handing the buffer to any call (`Buffer.clear(this.items)`, `zero(items)`)
            // delegates its cleanup — conservatively covered.
            for a in args {
                if let Some(t) = buffer_field_arg(a, fields, scan) {
                    scan.stored_buffers.push(t);
                }
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
                if let Some(t) = buffer_field_arg(a, fields, scan) {
                    scan.stored_buffers.push(t);
                }
                scan_expression(a, fields, siblings, scan);
            }
        }
        ExpressionNode::IndexAccess(base, index) => {
            // Reads are not stores: `let v = items[i]` does not release anything. Only
            // IndexAssignment / MemberAssignment count as covering.
            scan_expression(base, fields, siblings, scan);
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

/// Records an indexed store when `base` names a buffer field: directly (`items[i]`), through
/// `this` (`this.items[i]`), or into a slot member (`this.slots[i].value = v`, whose target is
/// an IndexAccess over the buffer).
fn note_buffer_store(base: &ExpressionNode, fields: &ContainerFields, scan: &mut MethodScan) {
    let name = match base {
        ExpressionNode::Identifier(t) => Some(&t.text),
        ExpressionNode::MemberAccess(recv, member) => {
            matches!(*recv, ExpressionNode::Identifier(t) if t.text == "this")
                .then_some(&member.text)
        }
        ExpressionNode::IndexAccess(inner_base, _) => {
            match &**inner_base {
                ExpressionNode::Identifier(t) => Some(&t.text),
                ExpressionNode::MemberAccess(recv, member) => {
                    matches!(*recv, ExpressionNode::Identifier(t) if t.text == "this")
                        .then_some(&member.text)
                }
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(name) = name {
        if scan.is_buffer_name(fields, name) {
            scan.stored_buffers.push(name.to_string());
        }
    }
}

/// `this.count = …` / bare `count = …` both write the counter inside a method body. Only
/// writes that can *shrink* the count matter — growth (`count + 1`, `count = n`) cannot strand
/// dead slots, so `push`-style methods stay silent.
fn note_counter_write(
    name: &str,
    value: Option<&ExpressionNode>,
    fields: &ContainerFields,
    scan: &mut MethodScan,
) {
    if !fields.counters.iter().any(|c| c == name) {
        return;
    }
    let shrinking = match value {
        Some(ExpressionNode::Literal(Type::Integer(tok))) => tok.text.parse::<i64>().unwrap_or(0) <= 0,
        Some(ExpressionNode::Binary(lhs, op, _)) => {
            (op.text == "-" || op.text == "-=") && lhs_refers_to(lhs, name)
        }
        // Bare counter reassignment with no visible RHS (`count = x;` where the parser folded
        // the expression elsewhere): treat as shrinking to stay conservative.
        None => true,
        _ => false,
    };
    if shrinking && !scan.count_writes.iter().any(|w| w == name) {
        scan.count_writes.push(name.to_string());
    }
}

/// True when the expression is (or reads through) the named counter, so `count - k` counts as
/// a shrinking write while `n - k` does not.
fn lhs_refers_to(e: &ExpressionNode, name: &str) -> bool {
    match e {
        ExpressionNode::Identifier(t) => t.text == name,
        ExpressionNode::MemberAccess(base, member) => {
            member.text == name
                && matches!(*base, ExpressionNode::Identifier(t) if t.text == "this")
        }
        ExpressionNode::Parenthesized(_, inner) => lhs_refers_to(inner, name),
        _ => false,
    }
}

/// Replacing the whole buffer field (`this.items = Buffer.alloc<T>(n)`) releases every old slot
/// through array-drop glue, so it covers any counter write in the same method.
fn note_buffer_reassign(name: &str, fields: &ContainerFields, scan: &mut MethodScan) {
    if fields.buffers.iter().any(|b| b == name) {
        scan.stored_buffers.push(name.to_string());
    }
}

/// `count--` / `--this.count`: the target is the field itself, not a member assignment.
fn note_field_incdec(target: &ExpressionNode, fields: &ContainerFields, scan: &mut MethodScan) {
    let name = match target {
        ExpressionNode::Identifier(t) => Some(&t.text),
        ExpressionNode::MemberAccess(recv, member) => {
            matches!(*recv, ExpressionNode::Identifier(t) if t.text == "this")
                .then_some(&member.text)
        }
        _ => None,
    };
    if let Some(name) = name {
        note_counter_write(name, None, fields, scan);
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

/// Buffer identity when `e` is a bare buffer-field reference (`items`, `this.items`, or a
/// local alias of one).
fn buffer_field_arg(e: &ExpressionNode, fields: &ContainerFields, scan: &MethodScan) -> Option<String> {
    let name = match e {
        ExpressionNode::Identifier(t) => Some(&t.text),
        ExpressionNode::MemberAccess(recv, member) => {
            matches!(*recv, ExpressionNode::Identifier(ref t) if t.text == "this")
                .then_some(&member.text)
        }
        _ => None,
    };
    let name = name?;
    if scan.is_buffer_name(fields, name) {
        Some(name.to_string())
    } else {
        None
    }
}

/// True when every generic parameter of the class is bound `unmanaged`.
fn class_is_unmanaged(decl: &StructDeclarationNode) -> bool {
    decl.generic_constraints
        .iter()
        .any(|c| c.kinds.iter().any(|k| matches!(k, dream_syntax::nodes::ConstraintKind::Unmanaged)))
}

/// True when the method carries a `where T : unmanaged` attachment constraint.
fn method_has_unmanaged_bound(method: &dream_syntax::nodes::function::FunctionNode) -> bool {
    method
        .where_constraints
        .iter()
        .any(|c| c.kinds.iter().any(|k| matches!(k, dream_syntax::nodes::ConstraintKind::Unmanaged)))
}
