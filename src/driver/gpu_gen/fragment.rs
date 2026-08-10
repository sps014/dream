//! `@fragment` shader emission.

use super::context::EmitCtx;
use super::helpers::emit_helpers_wgsl;
use super::ident::escape_wgsl_ident;
use super::layout::{
    build_struct_field_tys, emit_fragment_out_struct_wgsl, emit_interface_struct_wgsl, find_struct,
    fragment_color_target_count, has_position_gpuvec4, struct_name_of,
};
use super::stmt::emit_stmts;
use super::types::GpuShaderInfo;
use super::vertex::{emit_resource_param, finalize_uniforms};
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::function::FunctionNode;
use dream_syntax::nodes::types::Type;
use dream_syntax::nodes::ProgramNode;
use indexmap::IndexMap;
use std::cell::RefCell;

pub(super) fn emit_fragment(
    func: &FunctionNode<'_>,
    program: &ProgramNode<'_>,
    diagnostics: &mut DiagnosticBag,
) -> GpuShaderInfo {
    let name = func.name.text.clone();
    let entry = format!("dream_{}", name);

    let mut interface_ty = String::new();
    let mut struct_header = String::new();
    let mut bindings = Vec::new();
    let mut binding_idx = 0u32;
    let mut header = String::new();
    let mut uniform_fields = String::new();
    let mut has_uniform = false;
    let mut vary_param: Option<(String, String)> = None;
    let mut return_ty_wgsl = "@location(0) vec4<f32>".to_string();
    let mut color_targets = 1u32;

    match &func.return_type {
        Some(Type::Struct(tok, None)) if tok.text == "GpuVec4" => {
            return_ty_wgsl = "@location(0) vec4<f32>".to_string();
            color_targets = 1;
        }
        Some(Type::Struct(tok, None)) => {
            if let Some(decl) = find_struct(program, &tok.text) {
                match emit_fragment_out_struct_wgsl(decl) {
                    Ok((s, sname)) => {
                        struct_header.push_str(&s);
                        return_ty_wgsl = sname;
                        color_targets = fragment_color_target_count(decl);
                    }
                    Err(e) => diagnostics.report_error(e, Some(func.name.position)),
                }
            } else {
                diagnostics.report_error(
                    format!(
                        "@fragment '{}' return type '{}' is not a known struct",
                        name, tok.text
                    ),
                    Some(func.name.position),
                );
            }
        }
        _ => {
            diagnostics.report_error(
                format!(
                    "@fragment '{}' must return GpuVec4 or an unmanaged output struct",
                    name
                ),
                Some(func.name.position),
            );
        }
    }

    let mut param_iter = func.parameters.iter();
    if let Some(first) = param_iter.next() {
        let consumed = if let Some(sname) = struct_name_of(&first.type_) {
            if !matches!(sname, "GpuTexture" | "GpuSampler" | "GpuVec4") {
                if let Some(decl) = find_struct(program, sname) {
                    if !has_position_gpuvec4(decl) {
                        diagnostics.report_error(
                            format!(
                                "@fragment '{}' input struct '{}' must include a position builtin (field named `position: GpuVec4` or `@builtin(\"position\")`)",
                                name, sname
                            ),
                            Some(first.name.position),
                        );
                    }
                    interface_ty = sname.to_string();
                    vary_param = Some((first.name.text.clone(), sname.to_string()));
                    match emit_interface_struct_wgsl(decl, true) {
                        Ok(s) => struct_header.push_str(&s),
                        Err(e) => diagnostics.report_error(e, Some(func.name.position)),
                    }
                    true
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
            emit_resource_param(
                first,
                &entry,
                &mut header,
                &mut bindings,
                &mut binding_idx,
                &mut uniform_fields,
                &mut has_uniform,
            );
        }
    }

    for param in param_iter {
        emit_resource_param(
            param,
            &entry,
            &mut header,
            &mut bindings,
            &mut binding_idx,
            &mut uniform_fields,
            &mut has_uniform,
        );
    }

    if has_uniform {
        finalize_uniforms(
            &entry,
            binding_idx,
            &uniform_fields,
            &mut header,
            &mut bindings,
        );
    }

    let struct_fields = build_struct_field_tys(program);
    let helper_returns = super::helpers::build_helper_return_tys(program);
    let mut scopes = vec![IndexMap::new()];
    if let Some((ref vp, ref sname)) = vary_param {
        scopes[0].insert(vp.clone(), sname.clone());
    }
    let ctx = EmitCtx {
        prefix: &entry,
        bindings: &bindings,
        workgroup_names: &[],
        scopes: RefCell::new(scopes),
        struct_fields: &struct_fields,
        helper_returns: &helper_returns,
    };

    let mut workgroup_decls = String::new();
    let mut body = String::new();
    emit_stmts(
        func.body,
        &mut body,
        &mut workgroup_decls,
        1,
        &ctx,
        diagnostics,
        &func.name.text,
    );

    let helpers = emit_helpers_wgsl(func.body, program, diagnostics);

    let uses_sample_index = body_mentions(func.body, "sample_index");
    let uses_primitive_index = body_mentions(func.body, "primitive_index");

    let mut wgsl = String::new();
    if uses_primitive_index {
        wgsl.push_str("enable primitive_index;\n\n");
    }
    wgsl.push_str(&struct_header);
    wgsl.push_str(&helpers);
    wgsl.push_str(&header);
    wgsl.push('\n');
    wgsl.push_str(&format!("@fragment\nfn {entry}(\n"));
    wgsl.push_str("  @builtin(position) frag_coord: vec4<f32>,\n");
    wgsl.push_str("  @builtin(front_facing) front_facing: bool,\n");
    if uses_sample_index {
        wgsl.push_str("  @builtin(sample_index) _sample_index: u32,\n");
    }
    if uses_primitive_index {
        wgsl.push_str("  @builtin(primitive_index) _primitive_index: u32,\n");
    }
    if let Some((ref vp, ref sname)) = vary_param {
        wgsl.push_str(&format!(
            "  {}: {},\n",
            escape_wgsl_ident(vp),
            escape_wgsl_ident(sname)
        ));
    }
    wgsl.push_str(&format!(") -> {return_ty_wgsl} {{\n"));
    if uses_sample_index {
        wgsl.push_str("  let sample_index = i32(_sample_index);\n");
    }
    if uses_primitive_index {
        wgsl.push_str("  let primitive_index = i32(_primitive_index);\n");
    }
    wgsl.push_str(&body);
    wgsl.push_str("}\n");

    GpuShaderInfo {
        name,
        stage: "fragment",
        entry,
        bindings,
        vertex_layout: Vec::new(),
        vertex_stride: 0,
        interface_ty,
        color_targets,
        wgsl,
    }
}

fn body_mentions(stmts: &[dream_syntax::nodes::statement::StatementNode<'_>], name: &str) -> bool {
    // Debug formatting includes `text: "ident"` for every identifier token in the tree.
    format!("{stmts:?}").contains(&format!("text: \"{name}\""))
}
