//! `@vertex` shader emission.

use super::bind::{emit_resource_param, finalize_uniforms, BindingAlloc};
use super::context::EmitCtx;
use super::helpers::emit_helpers_wgsl;
use super::ident::escape_wgsl_ident;
use super::layout::{
    assign_locations, build_struct_field_tys, build_vertex_layout, dream_ty_to_wgsl_vec,
    emit_interface_struct_wgsl, find_struct, has_position_gpuvec4, struct_name_of,
};
use super::stmt::{emit_stmts, reject_gpu_nameof};
use super::types::GpuShaderInfo;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::function::FunctionNode;
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::types::Type;
use dream_syntax::nodes::ProgramNode;
use indexmap::IndexMap;
use std::cell::RefCell;

pub(super) fn emit_vertex(
    func: &FunctionNode<'_>,
    program: &ProgramNode<'_>,
    diagnostics: &mut DiagnosticBag,
) -> GpuShaderInfo {
    let name = func.name.text.clone();
    let entry = format!("dream_{}", name);

    let mut vertex_layout = Vec::new();
    let mut vertex_stride = 0u32;
    let mut interface_ty = String::new();
    let mut struct_header = String::new();
    let mut bindings = Vec::new();
    let mut alloc = BindingAlloc::default();
    let mut header = String::new();
    let mut uniform_fields = String::new();
    let mut has_uniform = false;
    let mut vertex_param: Option<(String, String)> = None; // (param_name, struct_name)

    let mut param_iter = func.parameters.iter();
    if let Some(first) = param_iter.next() {
        let consumed = if let Some(sname) = struct_name_of(&first.type_) {
            if !matches!(sname, "GpuTexture" | "GpuSampler") {
                if let Some(decl) = find_struct(program, sname) {
                    match build_vertex_layout(decl) {
                        Ok((layout, stride)) => {
                            vertex_layout = layout;
                            vertex_stride = stride;
                            vertex_param = Some((first.name.text.clone(), sname.to_string()));
                            match emit_vertex_in_struct(decl) {
                                Ok(s) => struct_header.push_str(&s),
                                Err(e) => diagnostics.report_error(e, Some(func.name.position)),
                            }
                            true
                        }
                        Err(e) => {
                            diagnostics.report_error(e, Some(first.name.position));
                            true
                        }
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        if !consumed {
            // First param is a resource/uniform, not vertex attrs.
            if let Err(e) = emit_resource_param(
                first,
                &entry,
                &mut header,
                &mut bindings,
                &mut alloc,
                &mut uniform_fields,
                &mut has_uniform,
            ) {
                diagnostics.report_error(e, Some(first.name.position));
            }
        }
    }

    for param in param_iter {
        if let Err(e) = emit_resource_param(
            param,
            &entry,
            &mut header,
            &mut bindings,
            &mut alloc,
            &mut uniform_fields,
            &mut has_uniform,
        ) {
            diagnostics.report_error(e, Some(param.name.position));
        }
    }

    if has_uniform {
        finalize_uniforms(
            &entry,
            &mut alloc,
            &uniform_fields,
            &mut header,
            &mut bindings,
        );
    }

    if let Some(Type::Struct(tok, None)) = &func.return_type {
        interface_ty = tok.text.clone();
        if let Some(decl) = find_struct(program, &tok.text) {
            if !has_position_gpuvec4(decl) {
                diagnostics.report_error(
                    format!(
                        "@vertex '{}' return struct '{}' must have a 'position: GpuVec4' field",
                        name, tok.text
                    ),
                    Some(func.name.position),
                );
            }
            match emit_interface_struct_wgsl(decl, false) {
                Ok(s) => struct_header.push_str(&s),
                Err(e) => diagnostics.report_error(e, Some(func.name.position)),
            }
        } else {
            diagnostics.report_error(
                format!(
                    "@vertex '{}' return type '{}' is not a known struct",
                    name, tok.text
                ),
                Some(func.name.position),
            );
        }
    } else {
        diagnostics.report_error(
            format!(
                "@vertex '{}' must return a value struct with a 'position: GpuVec4' field",
                name
            ),
            Some(func.name.position),
        );
    }

    let struct_fields = build_struct_field_tys(program);
    let helper_returns = super::helpers::build_helper_return_tys(program);
    let mut scopes = vec![IndexMap::new()];
    scopes[0].insert("vertex_index".into(), "i32".into());
    scopes[0].insert("instance_index".into(), "i32".into());
    if let Some((ref vp, ref sname)) = vertex_param {
        scopes[0].insert(vp.clone(), sname.clone());
    }
    let mut workgroup_decls = String::new();
    let mut body = String::new();
    {
        let ctx = EmitCtx {
            prefix: &entry,
            bindings: &bindings,
            workgroup_names: &[],
            scopes: RefCell::new(scopes),
            struct_fields: &struct_fields,
            helper_returns: &helper_returns,
            kernel: &func.name.text,
            diagnostics: RefCell::new(diagnostics),
        };
        reject_gpu_nameof(func.body, &ctx);
        emit_stmts(func.body, &mut body, &mut workgroup_decls, 1, &ctx);
    }

    let helpers = emit_helpers_wgsl(func.body, program, diagnostics);

    let mut wgsl = String::new();
    wgsl.push_str(&struct_header);
    wgsl.push_str(&helpers);
    wgsl.push_str(&header);
    wgsl.push('\n');
    wgsl.push_str(&format!("@vertex\nfn {entry}(\n"));
    // WGSL forbids identifiers starting with `__` (reserved); use a single-underscore prefix.
    wgsl.push_str("  @builtin(vertex_index) _vi: u32,\n");
    wgsl.push_str("  @builtin(instance_index) _ii: u32,\n");
    if let Some((ref vp, ref sname)) = vertex_param {
        wgsl.push_str(&format!(
            "  {}: {},\n",
            escape_wgsl_ident(vp),
            escape_wgsl_ident(sname)
        ));
    }
    let ret = if interface_ty.is_empty() {
        "void".into()
    } else {
        escape_wgsl_ident(&interface_ty)
    };
    wgsl.push_str(&format!(") -> {ret} {{\n"));
    wgsl.push_str("  let vertex_index = i32(_vi);\n");
    wgsl.push_str("  let instance_index = i32(_ii);\n");
    wgsl.push_str(&body);
    wgsl.push_str("}\n");

    GpuShaderInfo {
        name,
        stage: "vertex",
        entry,
        bindings,
        vertex_layout,
        vertex_stride,
        interface_ty,
        color_targets: 0,
        wgsl,
    }
}

fn emit_vertex_in_struct(decl: &StructDeclarationNode<'_>) -> Result<String, String> {
    let locs = assign_locations(decl)?;
    let sname = escape_wgsl_ident(&decl.name.text);
    let mut s = format!("struct {sname} {{\n");
    for field in &decl.fields {
        let Some(wgsl_ty) = dream_ty_to_wgsl_vec(&field.field_type) else {
            return Err(format!(
                "vertex field '{}' has unsupported type '{}'",
                field.name.text,
                field.field_type.get_type()
            ));
        };
        let loc = locs.get(&field.name.text).copied().unwrap_or(0);
        s.push_str(&format!(
            "  @location({loc}) {}: {wgsl_ty},\n",
            escape_wgsl_ident(&field.name.text)
        ));
    }
    s.push_str("}\n");
    Ok(s)
}

