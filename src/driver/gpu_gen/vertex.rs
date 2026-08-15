//! `@vertex` shader emission.

use super::context::EmitCtx;
use super::helpers::emit_helpers_wgsl;
use super::ident::escape_wgsl_ident;
use super::layout::{
    assign_locations, build_struct_field_tys, build_vertex_layout, dream_ty_to_wgsl_vec,
    emit_interface_struct_wgsl, find_struct, has_position_gpuvec4, struct_name_of,
};
use super::stmt::{emit_stmts, reject_gpu_nameof};
use super::ty::dream_ty_to_wgsl;
use super::types::{GpuBinding, GpuShaderInfo};
use dream_abi::attributes::{
    has_named_attr, has_readonly_attr, param_binding_override, param_group_override,
};
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::function::{FunctionNode, ParameterNode};
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
    let mut binding_idx = 0u32;
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
                &mut binding_idx,
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
            &mut binding_idx,
            &mut uniform_fields,
            &mut has_uniform,
        ) {
            diagnostics.report_error(e, Some(param.name.position));
        }
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

pub(super) enum ResClass {
    Texture { storage: bool },
    Sampler,
    Storage { elem: String },
    Uniform { ty: String },
}

pub(super) fn classify_resource(param: &ParameterNode) -> ResClass {
    let readonly = has_readonly_attr(&param.attributes);
    match &param.type_ {
        Type::Struct(tok, None) if tok.text == "GpuTexture" => {
            ResClass::Texture { storage: !readonly }
        }
        Type::Struct(tok, None) if tok.text == "GpuSampler" => ResClass::Sampler,
        Type::Struct(tok, Some(args)) if tok.text == "GpuBuffer" && args.len() == 1 => {
            ResClass::Storage {
                elem: dream_ty_to_wgsl(&args[0]),
            }
        }
        other => ResClass::Uniform {
            ty: dream_ty_to_wgsl(other),
        },
    }
}

pub(super) fn next_binding_slot(
    param: &ParameterNode,
    binding_idx: &mut u32,
) -> Result<(u32, u32), String> {
    let group = if has_named_attr(&param.attributes, "group") {
        param_group_override(&param.attributes).ok_or_else(|| {
            format!(
                "invalid @group on parameter '{}'; expected an integer literal",
                param.name.text
            )
        })?
    } else {
        0
    };
    let binding = match param_binding_override(&param.attributes) {
        Some(n) => n,
        None if has_named_attr(&param.attributes, "binding") => {
            return Err(format!(
                "invalid @binding on parameter '{}'; expected an integer literal",
                param.name.text
            ));
        }
        None => {
            let n = *binding_idx;
            *binding_idx += 1;
            n
        }
    };
    if param_binding_override(&param.attributes).is_some() {
        *binding_idx = (*binding_idx).max(binding + 1);
    }
    Ok((group, binding))
}

pub(super) fn emit_resource_param(
    param: &ParameterNode,
    entry: &str,
    header: &mut String,
    bindings: &mut Vec<GpuBinding>,
    binding_idx: &mut u32,
    uniform_fields: &mut String,
    has_uniform: &mut bool,
) -> Result<(), String> {
    match classify_resource(param) {
        ResClass::Texture { storage } => {
            let pname = param.name.text.clone();
            let wgsl_name = format!("{entry}_{pname}");
            let (group, binding) = next_binding_slot(param, binding_idx)?;
            let (kind, decl) = if storage {
                (
                    "storage_texture",
                    format!(
                        "@group({group}) @binding({binding}) var {wgsl_name}: texture_storage_2d<rgba8unorm, write>;\n"
                    ),
                )
            } else {
                (
                    "texture",
                    format!(
                        "@group({group}) @binding({binding}) var {wgsl_name}: texture_2d<f32>;\n"
                    ),
                )
            };
            header.push_str(&decl);
            bindings.push(GpuBinding {
                name: pname,
                binding,
                kind,
                wgsl_ty: if storage {
                    "texture_storage_2d<rgba8unorm, write>".into()
                } else {
                    "texture_2d<f32>".into()
                },
                read_write: storage,
                atomic: false,
            });
        }
        ResClass::Sampler => {
            let pname = param.name.text.clone();
            let wgsl_name = format!("{entry}_{pname}");
            let (group, binding) = next_binding_slot(param, binding_idx)?;
            header.push_str(&format!(
                "@group({group}) @binding({binding}) var {wgsl_name}: sampler;\n"
            ));
            bindings.push(GpuBinding {
                name: pname,
                binding,
                kind: "sampler",
                wgsl_ty: "sampler".into(),
                read_write: false,
                atomic: false,
            });
        }
        ResClass::Storage { elem } => {
            let pname = param.name.text.clone();
            let wgsl_name = format!("{entry}_{pname}");
            let (group, binding) = next_binding_slot(param, binding_idx)?;
            let elem_ty = escape_wgsl_ident(&elem);
            header.push_str(&format!(
                "@group({group}) @binding({binding}) var<storage, read> {wgsl_name}: array<{elem_ty}>;\n"
            ));
            bindings.push(GpuBinding {
                name: pname,
                binding,
                kind: "storage",
                wgsl_ty: elem,
                read_write: false,
                atomic: false,
            });
        }
        ResClass::Uniform { ty } => {
            *has_uniform = true;
            let pname = param.name.text.clone();
            uniform_fields.push_str(&format!("  {}: {ty},\n", escape_wgsl_ident(&pname)));
            let (_group, binding) = next_binding_slot(param, binding_idx)?;
            bindings.push(GpuBinding {
                name: pname,
                binding,
                kind: "uniform",
                wgsl_ty: ty,
                read_write: false,
                atomic: false,
            });
        }
    }
    Ok(())
}

pub(super) fn finalize_uniforms(
    entry: &str,
    binding_idx: u32,
    uniform_fields: &str,
    header: &mut String,
    bindings: &mut [GpuBinding],
) {
    let u_struct = format!("DreamUniforms_{entry}");
    let u_var = format!("dream_uniforms_{entry}");
    header.push_str(&format!("struct {u_struct} {{\n{uniform_fields}}}\n"));
    header.push_str(&format!(
        "@group(0) @binding({binding_idx}) var<uniform> {u_var}: {u_struct};\n"
    ));
    for b in bindings.iter_mut() {
        if b.kind == "uniform" {
            b.binding = binding_idx;
        }
    }
}
