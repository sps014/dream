//! GPU shader-stage, resource and vertex-layout attributes.

use super::*;

pub(super) const SPECS: &[AttributeSpec] = &[
    // WebGPU compute kernels: body is emitted as WGSL, not WASM. Optional 1–3 int args are the
    // workgroup size (X[, Y[, Z]]); bare `@compute` defaults to (64, 1, 1).
    AttributeSpec {
        name: "compute",
        targets: &[AttributeTarget::Function],
        args: ArgShape::Args {
            kinds: &[ArgKind::Int],
            min: 0,
            max: 3,
        },
        repeatable: false,
        doc: "Marks a function as a WebGPU compute kernel. Optional ints are workgroup size X[, Y[, Z]] (default 64, 1, 1).",
    },
    // Storage-buffer access mode for `@compute` params: WGSL `var<storage, read>` instead of
    // `read_write`. Only meaningful on `GpuBuffer<T>` kernel parameters.
    AttributeSpec {
        name: "readonly",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::None,
        repeatable: false,
        doc: "On a `@compute` `GpuBuffer` parameter: storage access is read-only (WGSL `read`).",
    },
    // Storage-texture opt-in: WGSL `texture_storage_*` instead of a sampled `texture_*`. Sampling
    // is the default because it is what render stages need.
    AttributeSpec {
        name: "storage",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::String, ArgKind::String],
            min: 0,
            max: 2,
        },
        repeatable: false,
        doc: "On a `GpuTexture` parameter: bind as a storage texture instead of a sampled one. Optional args are the texel format and access mode: `@storage(\"rgba8unorm\", \"write\")` (the default).",
    },
    // Texture view shape: picks the WGSL `texture_*` suffix and the layout's `viewDimension`.
    AttributeSpec {
        name: "view",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "On a `GpuTexture` parameter: how the binding views the texture's layers — `\"1d\"`, `\"2d\"` (the default), `\"2d-array\"`, `\"cube\"`, `\"cube-array\"`, or `\"3d\"`.",
    },
    // Depth-aspect sampled texture: WGSL `texture_depth_*`, the shadow-map read path.
    AttributeSpec {
        name: "depth",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::None,
        repeatable: false,
        doc: "On a `GpuTexture` parameter: bind the depth aspect (WGSL `texture_depth_2d`), readable with `Gpu.texture_sample_compare` against a `@compare` sampler.",
    },
    // Multisampled attachment read: WGSL `texture_multisampled_2d`, fetched with `textureLoad`.
    AttributeSpec {
        name: "multisampled",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::None,
        repeatable: false,
        doc: "On a `GpuTexture` parameter: bind a multisampled texture (WGSL `texture_multisampled_2d`). Such textures cannot be sampled, only `texture_load`ed per sample index.",
    },
    // Comparison sampler: WGSL `sampler_comparison`.
    AttributeSpec {
        name: "compare",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::None,
        repeatable: false,
        doc: "On a `GpuSampler` parameter: bind a comparison sampler (WGSL `sampler_comparison`) for `Gpu.texture_sample_compare`. The sampler must have been created with `GpuSamplerDesc.comparison`.",
    },
    // WebGPU vertex stage: body emitted as WGSL, not WASM.
    AttributeSpec {
        name: "vertex",
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a function as a WebGPU vertex shader. Body is emitted as WGSL, not WASM.",
    },
    // WebGPU fragment stage: body emitted as WGSL, not WASM.
    AttributeSpec {
        name: "fragment",
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a function as a WebGPU fragment shader. Body is emitted as WGSL, not WASM.",
    },
    // GPU helper: callable from `@compute`/`@vertex`/`@fragment` and emitted as a WGSL `fn`.
    AttributeSpec {
        name: "gpu",
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a helper callable from GPU shaders; body is also emitted as WGSL when referenced.",
    },
    // Stdlib shader builtin with no CPU meaning (atomics, derivatives, texture reads/writes).
    AttributeSpec {
        name: "shader_only",
        targets: &[AttributeTarget::StaticMethod],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a builtin that the WGSL emitter maps to a shader intrinsic and that has no CPU implementation; calls from CPU code are rejected.",
    },
    // Optional vertex/varying location remap; default is declaration order.
    AttributeSpec {
        name: "location",
        targets: &[AttributeTarget::Field],
        args: ArgShape::Args {
            kinds: &[ArgKind::Int],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Optional WGSL @location(N) override on a vertex-attribute or varying field (default: declaration order).",
    },
    // WGSL `@builtin(name)` on a struct field (e.g. `@builtin("position")`, `@builtin("frag_depth")`).
    AttributeSpec {
        name: "builtin",
        targets: &[AttributeTarget::Field],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Marks a shader I/O field as a WGSL builtin (e.g. `\"position\"`, `\"frag_depth\"`). A field named `position: GpuVec4` is still accepted as sugar for `@builtin(\"position\")`.",
    },
    // WGSL `@interpolate(type)` / `@interpolate(type, sampling)` on a varying field.
    AttributeSpec {
        name: "interpolate",
        targets: &[AttributeTarget::Field],
        args: ArgShape::Args {
            kinds: &[ArgKind::String, ArgKind::String],
            min: 1,
            max: 2,
        },
        repeatable: false,
        doc: "WGSL interpolation qualifier on a varying (`\"perspective\"`, `\"linear\"`, or `\"flat\"`), with an optional sampling qualifier: `\"centroid\"` or `\"sample\"` for the first two, `\"first\"` or `\"either\"` for `\"flat\"`.",
    },
    // Wire format of a vertex attribute, when it differs from the field's own type.
    AttributeSpec {
        name: "format",
        targets: &[AttributeTarget::Field],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Packed wire format of a vertex attribute (`\"unorm8x4\"`, `\"float16x2\"`, …). The buffer stores the packed bytes; the shader still sees the field's type.",
    },
    // Per-instance rather than per-vertex stepping for one vertex buffer.
    AttributeSpec {
        name: "instance",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::None,
        repeatable: false,
        doc: "Steps a `@vertex` struct parameter's buffer once per instance instead of once per vertex.",
    },
    // Explicit bind-group index override (default group 0).
    AttributeSpec {
        name: "group",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::Int],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Optional WGSL `@group(N)` override on a shader resource parameter (default: 0).",
    },
    // Explicit binding index override within a group.
    AttributeSpec {
        name: "binding",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::Int],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Optional WGSL `@binding(N)` override on a shader resource parameter (default: auto-assigned).",
    },
];

/// True when a parameter carries `@readonly` (compute storage → WGSL `read`).
pub fn has_readonly_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "readonly")
}

/// True when the declaration carries `@compute`.
pub fn has_compute_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "compute")
}

/// True when the declaration carries `@vertex`.
pub fn has_vertex_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "vertex")
}

/// True when the declaration carries `@fragment`.
pub fn has_fragment_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "fragment")
}

/// True when the declaration carries `@gpu` (shader-callable helper).
pub fn has_gpu_helper_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "gpu")
}

/// True when the declaration carries `@shader_only` (a WGSL builtin with no CPU implementation).
pub fn has_shader_only_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, "shader_only")
}

/// True when the declaration is any GPU shader stage (`@compute` / `@vertex` / `@fragment`).
pub fn is_gpu_shader_attr(attributes: &[AttributeNode]) -> bool {
    has_compute_attr(attributes) || has_vertex_attr(attributes) || has_fragment_attr(attributes)
}

/// Optional `@location(N)` on a struct field. `None` when absent or not a valid `u32`.
pub fn field_location_override(attributes: &[AttributeNode]) -> Option<u32> {
    parse_named_u32(attributes, "location")
}

/// Optional `@builtin("name")` on a struct field. `None` when absent or malformed.
pub fn field_builtin_name(attributes: &[AttributeNode]) -> Option<String> {
    let attr = attributes.iter().find(|a| a.name.text == "builtin")?;
    attr.args
        .first()
        .and_then(|t| t.as_string())
        .map(|s| s.to_string())
}

/// Optional `@format("name")` on a vertex-attribute field. `None` when absent or malformed.
pub fn field_vertex_format(attributes: &[AttributeNode]) -> Option<String> {
    let attr = attributes.iter().find(|a| a.name.text == "format")?;
    attr.args
        .first()
        .and_then(|t| t.as_string())
        .map(|s| s.to_string())
}

/// Optional `@interpolate("mode")` on a varying field. `None` when absent or malformed.
/// The `@interpolate(...)` mode on a varying field, with the arguments joined by commas.
///
/// WGSL takes an interpolation type and an optional sampling qualifier as two arguments, so
/// `@interpolate("perspective", "centroid")` reads back as `"perspective,centroid"` and the
/// emitter matches on the pair.
pub fn field_interpolate_mode(attributes: &[AttributeNode]) -> Option<String> {
    let attr = attributes.iter().find(|a| a.name.text == "interpolate")?;
    let parts: Vec<&str> = attr.args.iter().filter_map(|t| t.as_string()).collect();
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(","))
}

/// Optional `@group(N)` on a shader parameter. `None` when absent or not a valid `u32`.
pub fn param_group_override(attributes: &[AttributeNode]) -> Option<u32> {
    parse_named_u32(attributes, "group")
}

/// Optional `@binding(N)` on a shader parameter. `None` when absent or not a valid `u32`.
pub fn param_binding_override(attributes: &[AttributeNode]) -> Option<u32> {
    parse_named_u32(attributes, "binding")
}

/// True when a field is the clip-space position builtin (`@builtin("position")` or name `position`).
pub fn field_is_position_builtin(name: &str, attributes: &[AttributeNode]) -> bool {
    if let Some(b) = field_builtin_name(attributes) {
        return b == "position";
    }
    name == "position"
}

/// Workgroup size from `@compute` / `@compute(x[, y[, z]])`. Bare `@compute` is `(64, 1, 1)`.
/// Present arguments must parse as `u32` (decimal/hex/bin/oct); they are never silently defaulted.
pub fn compute_workgroup_size(attributes: &[AttributeNode]) -> Result<(u32, u32, u32), String> {
    let Some(attr) = attributes.iter().find(|a| a.name.text == "compute") else {
        return Ok((64, 1, 1));
    };
    if attr.args.is_empty() {
        return Ok((64, 1, 1));
    }
    let parse = |i: usize| -> Result<u32, String> {
        let arg = &attr.args[i];
        arg.as_int_text()
            .and_then(dream_syntax::number::parse_u32_literal)
            .ok_or_else(|| {
                format!(
                    "@compute workgroup size '{}' is not a valid u32",
                    arg.display()
                )
            })
    };
    match attr.args.len() {
        1 => Ok((parse(0)?, 1, 1)),
        2 => Ok((parse(0)?, parse(1)?, 1)),
        _ => Ok((parse(0)?, parse(1)?, parse(2)?)),
    }
}
