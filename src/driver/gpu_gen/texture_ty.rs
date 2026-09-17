//! Resolves a `GpuTexture` / `GpuSampler` shader parameter's attributes into the WGSL type it
//! declares and the bind-group-layout facts both hosts need.
//!
//! Every shape decision lives here so the compute and render parameter paths cannot disagree about
//! what `@view("cube") @depth` means.

use dream_abi::attributes::{has_named_attr, named_attr_string_arg};
use dream_abi::gpu_format::{self, FormatAspect, StorageAccess, ViewDimension};
use dream_syntax::nodes::function::ParameterNode;

/// A texture binding's WGSL type plus the layout metadata derived from it.
pub(super) struct TextureDecl {
    /// `"texture"` or `"storage_texture"`.
    pub kind: &'static str,
    pub wgsl_ty: String,
    /// WebGPU `viewDimension` spelling for the layout entry.
    pub view_dimension: &'static str,
    /// WebGPU `sampleType` spelling; empty for storage textures.
    pub sample_type: &'static str,
    pub multisampled: bool,
    /// Storage textures only: texel format and access, both empty otherwise.
    pub storage_format: &'static str,
    pub storage_access: &'static str,
    /// True when the shader may write through the binding.
    pub read_write: bool,
}

fn parse_view(param: &ParameterNode) -> Result<ViewDimension, String> {
    let Some(name) = named_attr_string_arg(&param.attributes, "view", 0) else {
        return Ok(ViewDimension::D2);
    };
    for dim in [
        ViewDimension::D1,
        ViewDimension::D2,
        ViewDimension::D2Array,
        ViewDimension::Cube,
        ViewDimension::CubeArray,
        ViewDimension::D3,
    ] {
        if dim.name() == name {
            return Ok(dim);
        }
    }
    Err(format!(
        "invalid @view(\"{name}\") on parameter '{}'; expected 1d, 2d, 2d-array, cube, cube-array, or 3d",
        param.name.text
    ))
}

/// A sampled texture: `texture_2d<f32>`, `texture_depth_2d`, `texture_multisampled_2d`, …
fn sampled(param: &ParameterNode, view: ViewDimension) -> Result<TextureDecl, String> {
    let depth = has_named_attr(&param.attributes, "depth");
    let multisampled = has_named_attr(&param.attributes, "multisampled");
    let suffix = view.wgsl_suffix();
    if multisampled && view != ViewDimension::D2 {
        return Err(format!(
            "@multisampled on parameter '{}' requires @view(\"2d\"); WGSL has no multisampled {suffix} texture",
            param.name.text
        ));
    }
    if depth && matches!(view, ViewDimension::D1 | ViewDimension::D3) {
        return Err(format!(
            "@depth on parameter '{}' cannot be combined with @view(\"{}\"); depth textures are 2d, 2d-array, cube, or cube-array",
            param.name.text,
            view.name()
        ));
    }
    let wgsl_ty = match (depth, multisampled) {
        (true, true) => "texture_depth_multisampled_2d".to_string(),
        (true, false) => format!("texture_depth_{suffix}"),
        (false, true) => "texture_multisampled_2d<f32>".to_string(),
        (false, false) => format!("texture_{suffix}<f32>"),
    };
    Ok(TextureDecl {
        kind: "texture",
        wgsl_ty,
        view_dimension: view.name(),
        sample_type: if depth { "depth" } else { "float" },
        multisampled,
        storage_format: "",
        storage_access: "",
        read_write: false,
    })
}

/// A storage texture: `texture_storage_2d<rgba8unorm, write>` and friends.
fn storage(param: &ParameterNode, view: ViewDimension) -> Result<TextureDecl, String> {
    let format_name = named_attr_string_arg(&param.attributes, "storage", 0).unwrap_or("rgba8unorm");
    let access_name = named_attr_string_arg(&param.attributes, "storage", 1).unwrap_or("write");
    let spec = gpu_format::by_name(format_name).ok_or_else(|| {
        format!(
            "invalid @storage format \"{format_name}\" on parameter '{}'; not a known texture format",
            param.name.text
        )
    })?;
    if !spec.storage {
        return Err(format!(
            "@storage format \"{format_name}\" on parameter '{}' cannot be a storage texture; WebGPU allows rgba8unorm, r16float, rg16float, rgba16float, r32float, rg32float, and rgba32float",
            param.name.text
        ));
    }
    let access = match access_name {
        "read" => StorageAccess::ReadOnly,
        "write" => StorageAccess::WriteOnly,
        "read_write" => StorageAccess::ReadWrite,
        other => {
            return Err(format!(
                "invalid @storage access \"{other}\" on parameter '{}'; expected read, write, or read_write",
                param.name.text
            ))
        }
    };
    // WebGPU only guarantees read-write storage textures for the 32-bit single-channel formats;
    // anything else needs an extension Dream does not request.
    if access == StorageAccess::ReadWrite && spec.bytes_per_texel != 4 {
        return Err(format!(
            "@storage read_write on parameter '{}' needs a 32-bit single-channel format like r32float, not \"{format_name}\"",
            param.name.text
        ));
    }
    if matches!(view, ViewDimension::Cube | ViewDimension::CubeArray) {
        return Err(format!(
            "@storage on parameter '{}' cannot use @view(\"{}\"); WGSL has no cube storage texture",
            param.name.text,
            view.name()
        ));
    }
    if has_named_attr(&param.attributes, "multisampled") {
        return Err(format!(
            "@storage on parameter '{}' cannot be @multisampled",
            param.name.text
        ));
    }
    if spec.aspect != FormatAspect::Color {
        return Err(format!(
            "@storage format \"{format_name}\" on parameter '{}' is a depth format",
            param.name.text
        ));
    }
    Ok(TextureDecl {
        kind: "storage_texture",
        wgsl_ty: format!(
            "texture_storage_{}<{}, {}>",
            view.wgsl_suffix(),
            spec.name,
            access.wgsl()
        ),
        view_dimension: view.name(),
        sample_type: "",
        multisampled: false,
        storage_format: spec.name,
        storage_access: access.name(),
        read_write: access != StorageAccess::ReadOnly,
    })
}

/// The WGSL declaration for a `GpuTexture` parameter, from `@view` / `@storage` / `@depth` /
/// `@multisampled`.
pub(super) fn texture_decl(param: &ParameterNode) -> Result<TextureDecl, String> {
    let view = parse_view(param)?;
    if has_named_attr(&param.attributes, "storage") {
        if has_named_attr(&param.attributes, "depth") {
            return Err(format!(
                "parameter '{}' cannot be both @storage and @depth",
                param.name.text
            ));
        }
        storage(param, view)
    } else {
        sampled(param, view)
    }
}

/// `sampler` or `sampler_comparison`, from `@compare`.
pub(super) fn sampler_wgsl_ty(param: &ParameterNode) -> &'static str {
    if has_named_attr(&param.attributes, "compare") {
        "sampler_comparison"
    } else {
        "sampler"
    }
}

/// WebGPU `samplerBindingType` for the layout entry.
pub(super) fn sampler_binding_type(param: &ParameterNode) -> &'static str {
    if has_named_attr(&param.attributes, "compare") {
        "comparison"
    } else {
        "filtering"
    }
}
