//! Emit WGSL helpers for ordinary Dream functions called from shader bodies.

use super::context::EmitCtx;
use super::ident::escape_wgsl_ident;
use super::layout::{build_struct_field_tys, find_struct};
use super::stmt::emit_stmts;
use super::ty::dream_ty_to_wgsl;
use dream_abi::attributes::{has_compute_attr, has_fragment_attr, has_vertex_attr};
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::expression::ExpressionNode;
use dream_syntax::nodes::function::FunctionNode;
use dream_syntax::nodes::statement::StatementNode;
use dream_syntax::nodes::ProgramNode;
use indexmap::{IndexMap, IndexSet};
use std::cell::RefCell;

const BUILTIN_CALLS: &[&str] = &[
    "workgroup_barrier",
    "storage_barrier",
    "atomic_load",
    "atomic_store",
    "atomic_add",
    "atomic_exchange",
    "texture_load",
    "texture_store",
    "texture_sample_level",
    "texture_sample",
    "of",
    "min",
    "max",
    "abs",
    "clamp",
    "sqrt",
    "floor",
    "ceil",
    "fract",
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "atan2",
    "normalize",
    "length",
    "dot",
    "cross",
    "reflect",
    "mix",
    "pow",
    "exp",
];

/// Collect free-function names called from `stmts` (excluding WGSL/Dream GPU builtins).
pub(super) fn collect_helper_calls(stmts: &[StatementNode<'_>], out: &mut IndexSet<String>) {
    for s in stmts {
        walk_stmt(s, out);
    }
}

fn walk_stmt(stmt: &StatementNode<'_>, out: &mut IndexSet<String>) {
    match stmt {
        StatementNode::ExpressionStatement(e)
        | StatementNode::Return(Some(e))
        | StatementNode::AwaitStmt(e)
        | StatementNode::Assignment(_, e)
        | StatementNode::Declaration(_, _, e, _)
        | StatementNode::MemberAssignment(_, _, e) => walk_expr(e, out),
        StatementNode::IndexAssignment(a, i, v) => {
            walk_expr(a, out);
            walk_expr(i, out);
            walk_expr(v, out);
        }
        StatementNode::FunctionInvocation(name, _, args) => {
            maybe_add(&name.text, out);
            for a in args {
                walk_expr(a, out);
            }
        }
        StatementNode::MethodInvocation(obj, method, _, args) => {
            if !BUILTIN_CALLS.contains(&method.text.as_str()) {
                maybe_add(&method.text, out);
            }
            walk_expr(obj, out);
            for a in args {
                walk_expr(a, out);
            }
        }
        StatementNode::IfElse(cond, then_b, elifs, else_b) => {
            walk_expr(cond, out);
            collect_helper_calls(then_b, out);
            for (c, b) in elifs {
                walk_expr(c, out);
                collect_helper_calls(b, out);
            }
            if let Some(eb) = else_b {
                collect_helper_calls(eb, out);
            }
        }
        StatementNode::While(cond, body) => {
            walk_expr(cond, out);
            collect_helper_calls(body, out);
        }
        StatementNode::DoWhile(body, cond) => {
            collect_helper_calls(body, out);
            walk_expr(cond, out);
        }
        StatementNode::For(init, cond, step, body) => {
            if let Some(i) = init {
                walk_stmt(i, out);
            }
            if let Some(c) = cond {
                walk_expr(c, out);
            }
            if let Some(s) = step {
                walk_stmt(s, out);
            }
            collect_helper_calls(body, out);
        }
        StatementNode::Labeled(_, inner) => walk_stmt(inner, out),
        StatementNode::Switch(subject, cases, default) => {
            walk_expr(subject, out);
            for (labels, body) in cases {
                for l in labels {
                    walk_expr(l, out);
                }
                collect_helper_calls(body, out);
            }
            if let Some(db) = default {
                collect_helper_calls(db, out);
            }
        }
        _ => {}
    }
}

fn walk_expr(expr: &ExpressionNode<'_>, out: &mut IndexSet<String>) {
    match expr {
        ExpressionNode::Binary(l, _, r) | ExpressionNode::IndexAccess(l, r) => {
            walk_expr(l, out);
            walk_expr(r, out);
        }
        ExpressionNode::MemberAccess(l, _) => walk_expr(l, out),
        ExpressionNode::Unary(_, e)
        | ExpressionNode::Parenthesized(_, e)
        | ExpressionNode::Cast(_, _, e)
        | ExpressionNode::NamedArg(_, e)
        | ExpressionNode::RefArgument(_, e)
        | ExpressionNode::IncDec { target: e, .. } => walk_expr(e, out),
        ExpressionNode::Ternary(c, t, e) => {
            walk_expr(c, out);
            walk_expr(t, out);
            walk_expr(e, out);
        }
        ExpressionNode::FunctionCall(name, _, args) => {
            maybe_add(&name.text, out);
            for a in args {
                walk_expr(a, out);
            }
        }
        ExpressionNode::MethodCall(obj, method, _, args) => {
            if !BUILTIN_CALLS.contains(&method.text.as_str()) {
                maybe_add(&method.text, out);
            }
            walk_expr(obj, out);
            for a in args {
                walk_expr(a, out);
            }
        }
        ExpressionNode::Call(callee, _, args) => {
            walk_expr(callee, out);
            for a in args {
                walk_expr(a, out);
            }
        }
        _ => {}
    }
}

fn maybe_add(name: &str, out: &mut IndexSet<String>) {
    if !BUILTIN_CALLS.contains(&name) {
        out.insert(name.to_string());
    }
}

/// `@gpu` free-function name → WGSL return type.
pub(super) fn build_helper_return_tys(
    program: &ProgramNode<'_>,
) -> IndexMap<String, String> {
    use super::ty::dream_ty_to_wgsl;
    let mut map = IndexMap::new();
    for f in &program.functions {
        if !dream_abi::attributes::has_gpu_helper_attr(&f.attributes) {
            continue;
        }
        if let Some(ret) = &f.return_type {
            map.insert(f.name.text.clone(), dream_ty_to_wgsl(ret));
        }
    }
    map
}

/// Emit WGSL `fn` definitions for helpers reachable from `entry_body`, in dependency order.
pub(super) fn emit_helpers_wgsl(
    entry_body: &[StatementNode<'_>],
    program: &ProgramNode<'_>,
    diagnostics: &mut DiagnosticBag,
) -> String {
    let mut needed = IndexSet::new();
    collect_helper_calls(entry_body, &mut needed);

    // Fixpoint: include transitive callees.
    let mut changed = true;
    while changed {
        changed = false;
        let snapshot: Vec<String> = needed.iter().cloned().collect();
        for name in snapshot {
            if let Some(func) = find_helper(program, &name) {
                let before = needed.len();
                collect_helper_calls(func.body, &mut needed);
                if needed.len() != before {
                    changed = true;
                }
            }
        }
    }

    // Topo emit: repeatedly emit a helper whose callees are already emitted.
    let struct_fields = build_struct_field_tys(program);
    let helper_returns = build_helper_return_tys(program);
    let mut emitted = IndexSet::new();
    let mut out = String::new();
    let mut guard = 0;
    while emitted.len() < needed.len() && guard < needed.len() + 2 {
        guard += 1;
        let mut progress = false;
        for name in needed.iter() {
            if emitted.contains(name) {
                continue;
            }
            let Some(func) = find_helper(program, name) else {
                // `VsOut()` etc. are struct constructors, not helpers.
                if find_struct(program, name).is_some() {
                    emitted.insert(name.clone());
                    progress = true;
                    continue;
                }
                diagnostics.report_error(
                    format!("GPU shader calls unknown helper function '{name}'"),
                    None,
                );
                emitted.insert(name.clone());
                progress = true;
                continue;
            };
            if has_vertex_attr(&func.attributes)
                || has_fragment_attr(&func.attributes)
                || has_compute_attr(&func.attributes)
            {
                emitted.insert(name.clone());
                progress = true;
                continue;
            }
            let mut deps = IndexSet::new();
            collect_helper_calls(func.body, &mut deps);
            if deps.iter().any(|d| needed.contains(d) && !emitted.contains(d)) {
                continue;
            }
            out.push_str(&emit_one_helper(
                func,
                &struct_fields,
                &helper_returns,
                diagnostics,
            ));
            emitted.insert(name.clone());
            progress = true;
        }
        if !progress {
            break;
        }
    }
    for name in &needed {
        if !emitted.contains(name) {
            diagnostics.report_error(
                format!("GPU helper '{name}' has a cyclic dependency"),
                None,
            );
        }
    }
    out
}

fn emit_one_helper(
    func: &FunctionNode<'_>,
    struct_fields: &IndexMap<String, IndexMap<String, String>>,
    helper_returns: &IndexMap<String, String>,
    diagnostics: &mut DiagnosticBag,
) -> String {
    let name = func.name.text.as_str();
    if func.generic_parameters.is_some() {
        diagnostics.report_error(
            format!("GPU helper '{name}' cannot be generic"),
            Some(func.name.position),
        );
        return String::new();
    }
    let ret = match &func.return_type {
        Some(ty) => dream_ty_to_wgsl(ty),
        None => {
            diagnostics.report_error(
                format!("GPU helper '{name}' must declare a non-void return type"),
                Some(func.name.position),
            );
            return String::new();
        }
    };

    let wgsl_name = escape_wgsl_ident(name);
    let mut params = Vec::new();
    let mut scopes = vec![IndexMap::new()];
    for p in &func.parameters {
        let pty = dream_ty_to_wgsl(&p.type_);
        let pname = escape_wgsl_ident(&p.name.text);
        scopes[0].insert(p.name.text.clone(), pty.clone());
        params.push(format!("{pname}: {pty}"));
    }

    let ctx = EmitCtx {
        prefix: &wgsl_name,
        bindings: &[],
        workgroup_names: &[],
        scopes: RefCell::new(scopes),
        struct_fields,
        helper_returns,
    };
    let mut body = String::new();
    let mut wg = String::new();
    emit_stmts(
        func.body,
        &mut body,
        &mut wg,
        1,
        &ctx,
        diagnostics,
        name,
    );
    if !wg.is_empty() {
        diagnostics.report_error(
            format!("GPU helper '{name}' cannot declare @workgroup locals"),
            Some(func.name.position),
        );
    }
    format!(
        "fn {wgsl_name}({}) -> {ret} {{\n{body}}}\n\n",
        params.join(", ")
    )
}

fn find_helper<'a>(program: &'a ProgramNode<'a>, name: &str) -> Option<&'a FunctionNode<'a>> {
    program.functions.iter().find(|f| {
        f.name.text == name && dream_abi::attributes::has_gpu_helper_attr(&f.attributes)
    })
}
