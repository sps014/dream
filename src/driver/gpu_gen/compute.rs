//! Single-kernel emission: bindings header, body, entry point wrapper.

use super::bind::{
    finalize_uniforms, next_binding_slot, sampler_binding, texture_binding, BindingAlloc,
};
use super::context::EmitCtx;
use super::ident::escape_wgsl_ident;
use super::layout::{build_struct_field_tys, emit_value_struct_wgsl, find_struct};
use super::stmt::{emit_stmts, reject_gpu_nameof};
use super::ty::dream_ty_to_wgsl;
use super::types::{GpuBinding, GpuKernelInfo};
use dream_abi::attributes::{compute_workgroup_size, has_readonly_attr};
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::expression::ExpressionNode;
use dream_syntax::nodes::function::{FunctionNode, ParameterNode};
use dream_syntax::nodes::statement::StatementNode;
use dream_syntax::nodes::types::Type;
use dream_syntax::nodes::ProgramNode;
use indexmap::{IndexMap, IndexSet};
use std::cell::RefCell;

fn collect_workgroup_names(stmts: &[StatementNode<'_>], out: &mut Vec<String>) {
    for s in stmts {
        match s {
            StatementNode::WorkgroupDecl(name, _, _) => out.push(name.text.clone()),
            StatementNode::IfElse(_, then_b, elifs, else_b) => {
                collect_workgroup_names(then_b, out);
                for (_, b) in elifs {
                    collect_workgroup_names(b, out);
                }
                if let Some(eb) = else_b {
                    collect_workgroup_names(eb, out);
                }
            }
            StatementNode::While(_, body) | StatementNode::DoWhile(body, _) => {
                collect_workgroup_names(body, out);
            }
            StatementNode::For(init, _, step, body) => {
                if let Some(i) = init {
                    collect_workgroup_names(std::slice::from_ref(i), out);
                }
                collect_workgroup_names(body, out);
                if let Some(st) = step {
                    collect_workgroup_names(std::slice::from_ref(st), out);
                }
            }
            StatementNode::Labeled(_, inner) => {
                collect_workgroup_names(std::slice::from_ref(inner), out);
            }
            StatementNode::Switch(_, cases, default) => {
                for (_, b) in cases {
                    collect_workgroup_names(b, out);
                }
                if let Some(db) = default {
                    collect_workgroup_names(db, out);
                }
            }
            _ => {}
        }
    }
}

pub(super) fn emit_kernel(
    func: &FunctionNode<'_>,
    program: &ProgramNode<'_>,
    diagnostics: &mut DiagnosticBag,
) -> GpuKernelInfo {
    let name = func.name.text.clone();
    let entry = format!("dream_{}", name);
    let workgroup = match compute_workgroup_size(&func.attributes) {
        Ok(w) => w,
        Err(e) => {
            diagnostics.report_error(e, Some(func.name.position));
            (64, 1, 1)
        }
    };
    let atomic_bufs = collect_atomic_buffer_names(func.body);

    let mut bindings = Vec::new();
    let mut alloc = BindingAlloc::default();
    let mut header = String::new();
    let mut uniforms: Vec<(String, String)> = Vec::new();

    let value_structs = collect_value_struct_names(func, program);
    for sname in &value_structs {
        if let Some(decl) = find_struct(program, sname) {
            match emit_value_struct_wgsl(decl) {
                Ok(s) => header.push_str(&s),
                Err(e) => diagnostics.report_error(e, Some(func.name.position)),
            }
        }
    }

    for param in &func.parameters {
        let pname = param.name.text.clone();
        let is_atomic = atomic_bufs.contains(pname.as_str());
        match classify_param(param, is_atomic) {
            ParamClass::Storage {
                elem,
                read_write,
                atomic,
            } => {
                let access = if read_write { "read_write" } else { "read" };
                let elem_ty = if atomic {
                    format!("atomic<{elem}>")
                } else {
                    escape_wgsl_ident(&elem)
                };
                let wgsl_name = format!("{entry}_{pname}");
                let (group, binding) = match next_binding_slot(param, &mut alloc) {
                    Ok(slot) => slot,
                    Err(e) => {
                        diagnostics.report_error(e, Some(param.name.position));
                        continue;
                    }
                };
                header.push_str(&format!(
                    "@group({group}) @binding({binding}) var<storage, {access}> {wgsl_name}: array<{elem_ty}>;\n"
                ));
                bindings.push(GpuBinding::buffer(
                    pname, group, binding, "storage", elem, read_write, atomic,
                ));
            }
            ParamClass::Texture => {
                let wgsl_name = format!("{entry}_{pname}");
                let (group, binding) = match next_binding_slot(param, &mut alloc) {
                    Ok(slot) => slot,
                    Err(e) => {
                        diagnostics.report_error(e, Some(param.name.position));
                        continue;
                    }
                };
                let b = match texture_binding(param, pname, group, binding) {
                    Ok(b) => b,
                    Err(e) => {
                        diagnostics.report_error(e, Some(param.name.position));
                        continue;
                    }
                };
                header.push_str(&format!(
                    "@group({group}) @binding({binding}) var {wgsl_name}: {};\n",
                    b.wgsl_ty
                ));
                bindings.push(b);
            }
            ParamClass::Sampler => {
                let wgsl_name = format!("{entry}_{pname}");
                let (group, binding) = match next_binding_slot(param, &mut alloc) {
                    Ok(slot) => slot,
                    Err(e) => {
                        diagnostics.report_error(e, Some(param.name.position));
                        continue;
                    }
                };
                let b = sampler_binding(param, pname, group, binding);
                header.push_str(&format!(
                    "@group({group}) @binding({binding}) var {wgsl_name}: {};\n",
                    b.wgsl_ty
                ));
                bindings.push(b);
            }
            ParamClass::Uniform { ty } => {
                if let Err(e) = alloc.note_uniform(param) {
                    diagnostics.report_error(e, Some(param.name.position));
                    continue;
                }
                uniforms.push((pname.clone(), ty.clone()));
                // group/binding are back-patched by `finalize_uniforms`
                bindings.push(GpuBinding::buffer(
                    pname, 0, 0, "uniform", ty, false, false,
                ));
            }
        }
    }

    let mut uniform_size = 0u32;
    if !uniforms.is_empty() {
        match finalize_uniforms(&entry, &mut alloc, &uniforms, &mut header, &mut bindings) {
            Ok(size) => uniform_size = size,
            Err(e) => diagnostics.report_error(e, Some(func.name.position)),
        }
    }

    let mut workgroup_names = Vec::new();
    collect_workgroup_names(func.body, &mut workgroup_names);
    let struct_fields = build_struct_field_tys(program);
    let helper_returns = super::helpers::build_helper_return_tys(program);
    let enum_values = super::layout::build_enum_values(program);
    // Shadowed as vec3<i32> in the prologue (`global_id_i`, …); type them for inference.
    let mut scopes = IndexMap::new();
    scopes.insert("local_invocation_index".to_string(), "i32".into());
    for name in ["global_id", "local_id", "workgroup_id", "num_workgroups"] {
        scopes.insert(name.to_string(), "vec3<i32>".into());
    }
    let ctx = EmitCtx {
        prefix: &entry,
        bindings: &bindings,
        workgroup_names: &workgroup_names,
        scopes: RefCell::new(vec![scopes]),
        struct_fields: &struct_fields,
        helper_returns: &helper_returns,
        enum_values: &enum_values,
        kernel: &func.name.text,
        diagnostics: RefCell::new(diagnostics),
    };

    let mut workgroup_decls = String::new();
    let mut body = String::new();
    reject_gpu_nameof(func.body, &ctx);
    emit_stmts(func.body, &mut body, &mut workgroup_decls, 1, &ctx);

    let helpers = super::helpers::emit_helpers_wgsl(func.body, program, &value_structs, diagnostics);

    let mut wgsl = String::new();
    wgsl.push_str(&header);
    wgsl.push('\n');
    // After the binding declarations, since a helper may read a storage buffer, and before the
    // entry point, because WGSL wants a function declared ahead of its callers.
    wgsl.push_str(&helpers);
    wgsl.push_str(&workgroup_decls);
    if !workgroup_decls.is_empty() {
        wgsl.push('\n');
    }
    wgsl.push_str(&format!(
        "@compute @workgroup_size({}, {}, {})\nfn {entry}(\n",
        workgroup.0, workgroup.1, workgroup.2
    ));
    wgsl.push_str("  @builtin(global_invocation_id) global_id: vec3<u32>,\n");
    wgsl.push_str("  @builtin(local_invocation_id) local_id: vec3<u32>,\n");
    wgsl.push_str("  @builtin(workgroup_id) workgroup_id: vec3<u32>,\n");
    wgsl.push_str("  @builtin(num_workgroups) num_workgroups: vec3<u32>,\n");
    wgsl.push_str("  @builtin(local_invocation_index) _lii: u32,\n");
    wgsl.push_str(") {\n");
    wgsl.push_str("  let local_invocation_index = i32(_lii);\n");
    // Shadow builtins as i32 structs so Dream-style `.x` field access typechecks in source
    // (`GpuId3`) while WGSL uses `vec3<u32>` builtins.
    wgsl.push_str(
        "  let global_id_i = vec3<i32>(i32(global_id.x), i32(global_id.y), i32(global_id.z));\n",
    );
    wgsl.push_str(
        "  let local_id_i = vec3<i32>(i32(local_id.x), i32(local_id.y), i32(local_id.z));\n",
    );
    wgsl.push_str(
        "  let workgroup_id_i = vec3<i32>(i32(workgroup_id.x), i32(workgroup_id.y), i32(workgroup_id.z));\n",
    );
    wgsl.push_str(
        "  let num_workgroups_i = vec3<i32>(i32(num_workgroups.x), i32(num_workgroups.y), i32(num_workgroups.z));\n",
    );
    // Remap Dream identifiers to the i32 vectors.
    let body_mapped = body
        .replace("global_id.", "global_id_i.")
        .replace("local_id.", "local_id_i.")
        .replace("workgroup_id.", "workgroup_id_i.")
        .replace("num_workgroups.", "num_workgroups_i.");
    wgsl.push_str(&body_mapped);
    wgsl.push_str("}\n");

    GpuKernelInfo {
        name,
        entry,
        workgroup,
        bindings,
        uniform_size,
        wgsl: super::intrinsic_wgsl::prepend_intrinsics(wgsl),
    }
}

enum ParamClass {
    Storage {
        elem: String,
        read_write: bool,
        atomic: bool,
    },
    /// Shape and access come from `@view` / `@storage` / `@depth` / `@multisampled`, resolved by
    /// `texture_ty::texture_decl`.
    Texture,
    Sampler,
    Uniform {
        ty: String,
    },
}

fn classify_param(param: &ParameterNode, is_atomic: bool) -> ParamClass {
    let readonly = has_readonly_attr(&param.attributes);
    match &param.type_ {
        // Bare `T[]` params are rejected in sema; only `GpuBuffer<T>` is storage.
        Type::Struct(tok, Some(args)) if tok.text == "GpuBuffer" && args.len() == 1 => {
            let elem = dream_ty_to_wgsl(&args[0]);
            let atomic = is_atomic && matches!(elem.as_str(), "i32" | "u32");
            ParamClass::Storage {
                elem,
                read_write: !readonly,
                atomic,
            }
        }
        Type::Struct(tok, None) if tok.text == "GpuTexture" => ParamClass::Texture,
        Type::Struct(tok, None) if tok.text == "GpuSampler" => ParamClass::Sampler,
        other => ParamClass::Uniform {
            ty: dream_ty_to_wgsl(other),
        },
    }
}

fn is_builtin_gpu_type(name: &str) -> bool {
    matches!(
        name,
        "GpuBuffer" | "GpuTexture" | "GpuSampler" | "GpuVec2" | "GpuVec3" | "GpuVec4" | "GpuId3"
    )
}

/// User value structs referenced as `GpuBuffer<T>` element types (need WGSL `struct` decls).
fn collect_value_struct_names(
    func: &FunctionNode<'_>,
    program: &ProgramNode<'_>,
) -> IndexSet<String> {
    let mut names = IndexSet::new();
    for param in &func.parameters {
        if let Type::Struct(tok, Some(args)) = &param.type_ {
            if tok.text == "GpuBuffer" {
                if let Some(Type::Struct(inner, None)) = args.first() {
                    if !is_builtin_gpu_type(&inner.text)
                        && find_struct(program, &inner.text).is_some()
                    {
                        names.insert(inner.text.clone());
                    }
                }
            }
        }
    }
    names
}

/// Buffer names passed to `Gpu.atomic_*` helpers — those storage bindings become `atomic<T>`.
fn collect_atomic_buffer_names(stmts: &[StatementNode<'_>]) -> IndexSet<String> {
    let mut out = IndexSet::new();
    walk_stmts_atomics(stmts, &mut out);
    out
}

fn walk_stmts_atomics(stmts: &[StatementNode<'_>], out: &mut IndexSet<String>) {
    for s in stmts {
        match s {
            StatementNode::ExpressionStatement(e)
            | StatementNode::Return(Some(e))
            | StatementNode::AwaitStmt(e) => walk_expr_atomics(e, out),
            StatementNode::Assignment(_, e)
            | StatementNode::Declaration(_, _, e, _)
            | StatementNode::MemberAssignment(_, _, e) => walk_expr_atomics(e, out),
            StatementNode::IndexAssignment(arr, idx, value) => {
                walk_expr_atomics(arr, out);
                walk_expr_atomics(idx, out);
                walk_expr_atomics(value, out);
            }
            StatementNode::FunctionInvocation(_, _, args) => {
                for a in args {
                    walk_expr_atomics(a, out);
                }
            }
            StatementNode::MethodInvocation(obj, method, _, args) => {
                if matches!(
                    method.text.as_str(),
                    "atomic_load"
                        | "atomic_add"
                        | "atomic_sub"
                        | "atomic_min"
                        | "atomic_max"
                        | "atomic_and"
                        | "atomic_or"
                        | "atomic_xor"
                        | "atomic_exchange"
                        | "atomic_store"
                ) {
                    if let Some(ExpressionNode::Identifier(name)) = args.first() {
                        out.insert(name.text.clone());
                    }
                }
                walk_expr_atomics(obj, out);
                for a in args {
                    walk_expr_atomics(a, out);
                }
            }
            StatementNode::IfElse(cond, then_b, elifs, else_b) => {
                walk_expr_atomics(cond, out);
                walk_stmts_atomics(then_b, out);
                for (c, body) in elifs {
                    walk_expr_atomics(c, out);
                    walk_stmts_atomics(body, out);
                }
                if let Some(eb) = else_b {
                    walk_stmts_atomics(eb, out);
                }
            }
            StatementNode::While(cond, body) => {
                walk_expr_atomics(cond, out);
                walk_stmts_atomics(body, out);
            }
            StatementNode::DoWhile(body, cond) => {
                walk_stmts_atomics(body, out);
                walk_expr_atomics(cond, out);
            }
            StatementNode::For(init, cond, step, body) => {
                if let Some(i) = init {
                    walk_stmts_atomics(std::slice::from_ref(i), out);
                }
                if let Some(c) = cond {
                    walk_expr_atomics(c, out);
                }
                if let Some(s) = step {
                    walk_stmts_atomics(std::slice::from_ref(s), out);
                }
                walk_stmts_atomics(body, out);
            }
            StatementNode::Switch(subj, cases, default) => {
                walk_expr_atomics(subj, out);
                for (_, body) in cases {
                    walk_stmts_atomics(body, out);
                }
                if let Some(d) = default {
                    walk_stmts_atomics(d, out);
                }
            }
            StatementNode::Labeled(_, inner) => {
                walk_stmts_atomics(std::slice::from_ref(inner), out)
            }
            StatementNode::Lock(e, body) => {
                walk_expr_atomics(e, out);
                walk_stmts_atomics(body, out);
            }
            StatementNode::Defer(budget, body) => {
                if let Some(q) = budget {
                    walk_expr_atomics(q, out);
                }
                walk_stmts_atomics(body, out);
            }
            _ => {}
        }
    }
}

fn walk_expr_atomics(expr: &ExpressionNode<'_>, out: &mut IndexSet<String>) {
    match expr {
        ExpressionNode::MethodCall(obj, method, _, args)
            if matches!(
                method.text.as_str(),
                "atomic_load"
                    | "atomic_add"
                    | "atomic_sub"
                    | "atomic_min"
                    | "atomic_max"
                    | "atomic_and"
                    | "atomic_or"
                    | "atomic_xor"
                    | "atomic_exchange"
                    | "atomic_store"
            ) =>
        {
            if let Some(ExpressionNode::Identifier(name)) = args.first() {
                out.insert(name.text.clone());
            }
            walk_expr_atomics(obj, out);
            for a in args {
                walk_expr_atomics(a, out);
            }
        }
        ExpressionNode::FunctionCall(name, _, args)
            if matches!(
                name.text.as_str(),
                "atomic_load"
                    | "atomic_add"
                    | "atomic_sub"
                    | "atomic_min"
                    | "atomic_max"
                    | "atomic_and"
                    | "atomic_or"
                    | "atomic_xor"
                    | "atomic_exchange"
                    | "atomic_store"
            ) =>
        {
            if let Some(ExpressionNode::Identifier(n)) = args.first() {
                out.insert(n.text.clone());
            }
            for a in args {
                walk_expr_atomics(a, out);
            }
        }
        ExpressionNode::Binary(l, _, r) | ExpressionNode::IndexAccess(l, r) => {
            walk_expr_atomics(l, out);
            walk_expr_atomics(r, out);
        }
        ExpressionNode::MemberAccess(l, _) => walk_expr_atomics(l, out),
        ExpressionNode::Ternary(c, t, e) => {
            walk_expr_atomics(c, out);
            walk_expr_atomics(t, out);
            walk_expr_atomics(e, out);
        }
        ExpressionNode::Unary(_, e)
        | ExpressionNode::Parenthesized(_, e)
        | ExpressionNode::Cast(_, _, e)
        | ExpressionNode::NamedArg(_, e)
        | ExpressionNode::RefArgument(_, e)
        | ExpressionNode::Await(_, e)
        | ExpressionNode::Try(e)
        | ExpressionNode::IncDec { target: e, .. } => walk_expr_atomics(e, out),
        ExpressionNode::FunctionCall(_, _, args) => {
            for a in args {
                walk_expr_atomics(a, out);
            }
        }
        ExpressionNode::MethodCall(obj, _, _, args) | ExpressionNode::Call(obj, _, args) => {
            walk_expr_atomics(obj, out);
            for a in args {
                walk_expr_atomics(a, out);
            }
        }
        _ => {}
    }
}
