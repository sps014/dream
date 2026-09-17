//! Shader resource parameters: bind-group/binding allocation and WGSL resource declarations.

use super::ident::escape_wgsl_ident;
use super::texture_ty::{sampler_binding_type, sampler_wgsl_ty, texture_decl};
use super::ty::dream_ty_to_wgsl;
use super::types::GpuBinding;
use dream_abi::attributes::{has_named_attr, param_binding_override, param_group_override};
use dream_syntax::nodes::function::ParameterNode;
use dream_syntax::nodes::types::Type;
use indexmap::IndexMap;

/// Binding indices are unique *within* a bind group, so auto-assignment keeps a counter per
/// group. All uniform parameters of one shader collapse into a single WGSL uniform block, so its
/// slot is resolved once at the end rather than per parameter.
#[derive(Default)]
pub(super) struct BindingAlloc {
    next: IndexMap<u32, u32>,
    uniform_group: Option<u32>,
    uniform_binding: Option<u32>,
}

impl BindingAlloc {
    fn reserve(&mut self, group: u32, binding: u32) {
        let slot = self.next.entry(group).or_insert(0);
        *slot = (*slot).max(binding + 1);
    }

    fn take_next(&mut self, group: u32) -> u32 {
        let slot = self.next.entry(group).or_insert(0);
        let n = *slot;
        *slot += 1;
        n
    }

    /// Records `@group`/`@binding` written on a uniform parameter. Every uniform parameter shares
    /// one block, so two parameters asking for different slots is a contradiction, not a merge.
    pub(super) fn note_uniform(&mut self, param: &ParameterNode) -> Result<(), String> {
        let (group, binding) = read_slot_attrs(param)?;
        if let Some(g) = group {
            if self.uniform_group.is_some_and(|prev| prev != g) {
                return Err(format!(
                    "conflicting @group on uniform parameter '{}'; all uniform parameters of one shader share a single block",
                    param.name.text
                ));
            }
            self.uniform_group = Some(g);
        }
        if let Some(b) = binding {
            if self.uniform_binding.is_some_and(|prev| prev != b) {
                return Err(format!(
                    "conflicting @binding on uniform parameter '{}'; all uniform parameters of one shader share a single block",
                    param.name.text
                ));
            }
            self.uniform_binding = Some(b);
        }
        Ok(())
    }

    fn uniform_slot(&mut self) -> (u32, u32) {
        let group = self.uniform_group.unwrap_or(0);
        match self.uniform_binding {
            Some(b) => {
                self.reserve(group, b);
                (group, b)
            }
            None => (group, self.take_next(group)),
        }
    }
}

fn read_slot_attrs(param: &ParameterNode) -> Result<(Option<u32>, Option<u32>), String> {
    let group = if has_named_attr(&param.attributes, "group") {
        Some(param_group_override(&param.attributes).ok_or_else(|| {
            format!(
                "invalid @group on parameter '{}'; expected an integer literal",
                param.name.text
            )
        })?)
    } else {
        None
    };
    let binding = if has_named_attr(&param.attributes, "binding") {
        Some(param_binding_override(&param.attributes).ok_or_else(|| {
            format!(
                "invalid @binding on parameter '{}'; expected an integer literal",
                param.name.text
            )
        })?)
    } else {
        None
    };
    Ok((group, binding))
}

pub(super) fn next_binding_slot(
    param: &ParameterNode,
    alloc: &mut BindingAlloc,
) -> Result<(u32, u32), String> {
    let (group_attr, binding_attr) = read_slot_attrs(param)?;
    let group = group_attr.unwrap_or(0);
    match binding_attr {
        Some(b) => {
            alloc.reserve(group, b);
            Ok((group, b))
        }
        None => Ok((group, alloc.take_next(group))),
    }
}

pub(super) enum ResClass {
    Texture,
    Sampler,
    Storage { elem: String },
    Uniform { ty: String },
}

pub(super) fn classify_resource(param: &ParameterNode) -> ResClass {
    match &param.type_ {
        Type::Struct(tok, None) if tok.text == "GpuTexture" => ResClass::Texture,
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

/// The `GpuBinding` for a `GpuTexture` parameter, shared by the render and compute paths.
pub(super) fn texture_binding(
    param: &ParameterNode,
    name: String,
    group: u32,
    binding: u32,
) -> Result<GpuBinding, String> {
    let decl = texture_decl(param)?;
    Ok(GpuBinding {
        name,
        group,
        binding,
        kind: decl.kind,
        wgsl_ty: decl.wgsl_ty,
        read_write: decl.read_write,
        atomic: false,
        view_dimension: decl.view_dimension,
        sample_type: decl.sample_type,
        multisampled: decl.multisampled,
        storage_format: decl.storage_format,
        storage_access: decl.storage_access,
    })
}

/// The `GpuBinding` for a `GpuSampler` parameter.
pub(super) fn sampler_binding(
    param: &ParameterNode,
    name: String,
    group: u32,
    binding: u32,
) -> GpuBinding {
    GpuBinding {
        name,
        group,
        binding,
        kind: "sampler",
        wgsl_ty: sampler_wgsl_ty(param).to_string(),
        read_write: false,
        atomic: false,
        view_dimension: "",
        sample_type: sampler_binding_type(param),
        multisampled: false,
        storage_format: "",
        storage_access: "",
    }
}

/// Emits one WGSL resource declaration for a `@vertex` / `@fragment` parameter.
pub(super) fn emit_resource_param(
    param: &ParameterNode,
    entry: &str,
    header: &mut String,
    bindings: &mut Vec<GpuBinding>,
    alloc: &mut BindingAlloc,
    uniform_fields: &mut String,
    has_uniform: &mut bool,
) -> Result<(), String> {
    let pname = param.name.text.clone();
    let wgsl_name = format!("{entry}_{pname}");
    match classify_resource(param) {
        ResClass::Texture => {
            let (group, binding) = next_binding_slot(param, alloc)?;
            let b = texture_binding(param, pname, group, binding)?;
            header.push_str(&format!(
                "@group({group}) @binding({binding}) var {wgsl_name}: {};\n",
                b.wgsl_ty
            ));
            bindings.push(b);
        }
        ResClass::Sampler => {
            let (group, binding) = next_binding_slot(param, alloc)?;
            let b = sampler_binding(param, pname, group, binding);
            header.push_str(&format!(
                "@group({group}) @binding({binding}) var {wgsl_name}: {};\n",
                b.wgsl_ty
            ));
            bindings.push(b);
        }
        ResClass::Storage { elem } => {
            let (group, binding) = next_binding_slot(param, alloc)?;
            let elem_ty = escape_wgsl_ident(&elem);
            header.push_str(&format!(
                "@group({group}) @binding({binding}) var<storage, read> {wgsl_name}: array<{elem_ty}>;\n"
            ));
            bindings.push(GpuBinding::buffer(
                pname,
                group,
                binding,
                "storage",
                elem,
                // Render stages only ever read storage buffers; `read_write` needs a kernel.
                false,
                false,
            ));
        }
        ResClass::Uniform { ty } => {
            *has_uniform = true;
            alloc.note_uniform(param)?;
            uniform_fields.push_str(&format!("  {}: {ty},\n", escape_wgsl_ident(&pname)));
            bindings.push(GpuBinding::buffer(pname, 0, 0, "uniform", ty, false, false));
        }
    }
    Ok(())
}

/// Declares the shared uniform block and back-patches every `uniform` binding to its slot.
pub(super) fn finalize_uniforms(
    entry: &str,
    alloc: &mut BindingAlloc,
    uniform_fields: &str,
    header: &mut String,
    bindings: &mut [GpuBinding],
) {
    let (group, binding) = alloc.uniform_slot();
    let u_struct = format!("DreamUniforms_{entry}");
    let u_var = format!("dream_uniforms_{entry}");
    header.push_str(&format!("struct {u_struct} {{\n{uniform_fields}}}\n"));
    header.push_str(&format!(
        "@group({group}) @binding({binding}) var<uniform> {u_var}: {u_struct};\n"
    ));
    for b in bindings.iter_mut() {
        if b.kind == "uniform" {
            b.group = group;
            b.binding = binding;
        }
    }
}
