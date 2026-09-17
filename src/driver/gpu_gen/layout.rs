//! Vertex attribute / varying location assignment and WGSL type helpers for render structs.

use dream_abi::attributes::{
    field_builtin_name, field_interpolate_mode, field_is_position_builtin, field_location_override,
    field_vertex_format, has_named_attr,
};
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::types::Type;
use dream_syntax::nodes::ProgramNode;
use indexmap::IndexMap;

use super::ident::escape_wgsl_ident;
use super::types::{GpuVertexAttr, GpuVertexBuffer};

pub(super) fn find_struct<'a>(
    program: &'a ProgramNode<'a>,
    name: &str,
) -> Option<&'a StructDeclarationNode<'a>> {
    program.structs.iter().find(|s| s.name.text == name)
}

pub(super) fn struct_name_of(ty: &Type) -> Option<&str> {
    match ty {
        Type::Struct(tok, None) => Some(tok.text.as_str()),
        _ => None,
    }
}

pub(super) fn dream_ty_to_wgsl_vec(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Float(_) | Type::Double(_) => Some("f32"),
        Type::Integer(_) | Type::Byte(_) | Type::Long(_) => Some("i32"),
        Type::UInt(_) | Type::ULong(_) => Some("u32"),
        Type::Boolean(_) => Some("bool"),
        Type::Struct(tok, None) => match tok.text.as_str() {
            "GpuVec2" => Some("vec2<f32>"),
            "GpuVec3" => Some("vec3<f32>"),
            "GpuVec4" => Some("vec4<f32>"),
            "GpuId3" => Some("vec3<i32>"),
            "GpuMat2" => Some("mat2x2<f32>"),
            "GpuMat3" => Some("mat3x3<f32>"),
            "GpuMat4" => Some("mat4x4<f32>"),
            _ => None,
        },
        _ => None,
    }
}

/// Struct → field → WGSL type map for member-access inference in shaders.
/// Enum name → member name → integer value, for folding `Mode.Mul` to a literal.
///
/// Only C-style enums are included: a variant carrying a payload is a discriminated union, which
/// has no shader representation, and folding it to its discriminant would quietly drop the
/// payload rather than reporting that the type does not belong in a shader.
pub(super) fn build_enum_values(
    program: &ProgramNode<'_>,
) -> IndexMap<String, IndexMap<String, i32>> {
    let mut map = IndexMap::new();
    for decl in &program.enums {
        if decl.variants.iter().any(|v| !v.fields.is_empty()) {
            continue;
        }
        let members = decl
            .variants
            .iter()
            .map(|v| (v.name.text.clone(), v.value))
            .collect();
        map.insert(decl.name.text.clone(), members);
    }
    map
}

pub(super) fn build_struct_field_tys(
    program: &ProgramNode<'_>,
) -> IndexMap<String, IndexMap<String, String>> {
    use super::ty::dream_ty_to_wgsl;

    let mut map = IndexMap::new();
    let mut insert_builtin = |name: &str, fields: &[(&str, &str)]| {
        let mut f = IndexMap::new();
        for (n, t) in fields {
            f.insert((*n).to_string(), (*t).to_string());
        }
        map.insert(name.to_string(), f);
    };
    insert_builtin("GpuVec2", &[("x", "f32"), ("y", "f32")]);
    insert_builtin("GpuVec3", &[("x", "f32"), ("y", "f32"), ("z", "f32")]);
    insert_builtin(
        "GpuVec4",
        &[("x", "f32"), ("y", "f32"), ("z", "f32"), ("w", "f32")],
    );
    insert_builtin("GpuId3", &[("x", "i32"), ("y", "i32"), ("z", "i32")]);
    insert_builtin("GpuMat2", &[("c0", "vec2<f32>"), ("c1", "vec2<f32>")]);
    insert_builtin(
        "GpuMat3",
        &[
            ("c0", "vec3<f32>"),
            ("c1", "vec3<f32>"),
            ("c2", "vec3<f32>"),
        ],
    );
    insert_builtin(
        "GpuMat4",
        &[
            ("c0", "vec4<f32>"),
            ("c1", "vec4<f32>"),
            ("c2", "vec4<f32>"),
            ("c3", "vec4<f32>"),
        ],
    );

    for st in &program.structs {
        let mut fields = IndexMap::new();
        for field in &st.fields {
            let ty = if field_builtin(field).as_deref() == Some(SAMPLE_MASK_BUILTIN) {
                // Declared `int` in Dream but `u32` in WGSL, so stores into it have to be
                // converted; reporting the field's own type here would emit an `i32` store.
                "u32".to_string()
            } else {
                dream_ty_to_wgsl_vec(&field.field_type)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| dream_ty_to_wgsl(&field.field_type))
            };
            fields.insert(field.name.text.clone(), ty);
        }
        map.insert(st.name.text.clone(), fields);
    }
    map
}

/// Default wire format for a field, when `@format` does not override it: the unpacked format
/// matching the field's own type.
pub(super) fn vertex_format_of(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Float(_) | Type::Double(_) => Some("float32"),
        Type::Integer(_) | Type::Byte(_) | Type::Long(_) => Some("sint32"),
        Type::UInt(_) | Type::ULong(_) => Some("uint32"),
        Type::Struct(tok, None) => match tok.text.as_str() {
            "GpuVec2" => Some("float32x2"),
            "GpuVec3" => Some("float32x3"),
            "GpuVec4" => Some("float32x4"),
            _ => None,
        },
        _ => None,
    }
}

/// Resolves a field's declared `@format` against the shared registry.
///
/// A packed format only changes how many bytes the buffer holds — the shader still reads the
/// field's own type — so the format's WGSL type has to be the one the field already has, or the
/// vertex stage would be reading something other than what was written.
fn packed_format(
    field: &dream_syntax::nodes::struct_node::StructFieldNode,
    declared: &str,
) -> Result<&'static str, String> {
    let spec = dream_abi::gpu_format::vertex_format_by_name(declared).ok_or_else(|| {
        format!(
            "unknown @format(\"{declared}\") on vertex attribute '{}'",
            field.name.text
        )
    })?;
    let wgsl = dream_ty_to_wgsl_vec(&field.field_type).ok_or_else(|| {
        format!(
            "vertex attribute '{}' has unsupported type '{}'",
            field.name.text,
            field.field_type.get_type()
        )
    })?;
    if spec.wgsl != wgsl {
        return Err(format!(
            "@format(\"{declared}\") reads as '{}' in a shader, but vertex attribute '{}' is declared '{}'",
            spec.wgsl,
            field.name.text,
            field.field_type.get_type()
        ));
    }
    Ok(spec.name)
}

fn vertex_attr_bytes(format: &str) -> u32 {
    dream_abi::gpu_format::vertex_format_by_name(format)
        .map(|s| s.size)
        .unwrap_or(4)
}

fn field_builtin(field: &dream_syntax::nodes::struct_node::StructFieldNode) -> Option<String> {
    if let Some(b) = field_builtin_name(&field.attributes) {
        return Some(b);
    }
    if field_is_position_builtin(&field.name.text, &field.attributes) {
        return Some("position".to_string());
    }
    None
}

/// WGSL pairs an interpolation type with an optional sampling qualifier. `centroid` and `sample`
/// only change anything under MSAA: they move the sample point inside the covered area, which
/// keeps interpolated values from being extrapolated off the edge of a partially covered
/// triangle, and `sample` additionally forces per-sample shading.
///
/// `flat` takes `first`/`either` instead, which pick the provoking vertex.
fn interpolate_wgsl(mode: &str) -> Option<&'static str> {
    match mode {
        "perspective" => Some("@interpolate(perspective)"),
        "linear" => Some("@interpolate(linear)"),
        "flat" => Some("@interpolate(flat)"),
        "perspective,centroid" => Some("@interpolate(perspective, centroid)"),
        "perspective,sample" => Some("@interpolate(perspective, sample)"),
        "linear,centroid" => Some("@interpolate(linear, centroid)"),
        "linear,sample" => Some("@interpolate(linear, sample)"),
        // Bare sampling qualifiers imply the default interpolation type.
        "centroid" => Some("@interpolate(perspective, centroid)"),
        "sample" => Some("@interpolate(perspective, sample)"),
        "flat,first" => Some("@interpolate(flat, first)"),
        "flat,either" => Some("@interpolate(flat, either)"),
        _ => None,
    }
}

/// Assign locations by declaration order, honoring optional `@location(N)` overrides.
/// Skips builtin fields (including `position` sugar).
pub(super) fn assign_locations(
    decl: &StructDeclarationNode<'_>,
) -> Result<IndexMap<String, u32>, String> {
    assign_locations_from(decl, 0)
}

/// Like [`assign_locations`], starting auto-assignment at `base`.
///
/// Locations are unique across the whole vertex stage, not per struct, so a second vertex buffer
/// continues where the first left off. An explicit `@location(N)` is absolute either way.
pub(super) fn assign_locations_from(
    decl: &StructDeclarationNode<'_>,
    base: u32,
) -> Result<IndexMap<String, u32>, String> {
    let mut map = IndexMap::new();
    let mut used: IndexMap<u32, String> = IndexMap::new();
    let mut auto = base;
    for field in &decl.fields {
        if field_builtin(field).is_some() {
            continue;
        }
        let loc = if has_named_attr(&field.attributes, "location") {
            field_location_override(&field.attributes).ok_or_else(|| {
                format!(
                    "invalid @location on field '{}'; expected an integer literal",
                    field.name.text
                )
            })?
        } else {
            while used.contains_key(&auto) {
                auto += 1;
            }
            auto
        };
        if let Some(prev) = used.insert(loc, field.name.text.clone()) {
            return Err(format!(
                "duplicate @location({}) on fields '{}' and '{}'",
                loc, prev, field.name.text
            ));
        }
        map.insert(field.name.text.clone(), loc);
        if field_location_override(&field.attributes).is_none() {
            auto = loc + 1;
        }
    }
    Ok(map)
}

/// One vertex buffer's attribute layout, derived from the struct a `@vertex` parameter takes.
///
/// Attributes are laid out tightly in declaration order, so the Dream-side struct the caller
/// uploads and the layout the pipeline declares agree by construction.
pub(super) fn build_vertex_layout(
    decl: &StructDeclarationNode<'_>,
    step_mode: &'static str,
    base_location: u32,
) -> Result<GpuVertexBuffer, String> {
    let locs = assign_locations_from(decl, base_location)?;
    let mut layout = Vec::new();
    let mut offset = 0u32;
    for field in &decl.fields {
        if field_builtin(field).is_some() {
            return Err(format!(
                "vertex attribute '{}' cannot be a @builtin; builtins belong on stage I/O structs",
                field.name.text
            ));
        }
        let format = match field_vertex_format(&field.attributes) {
            Some(declared) => packed_format(field, &declared)?,
            None => vertex_format_of(&field.field_type).ok_or_else(|| {
                format!(
                    "vertex attribute '{}' has unsupported type '{}'; use float, int, uint, or GpuVec2/3/4",
                    field.name.text,
                    field.field_type.get_type()
                )
            })?,
        };
        let location = *locs.get(&field.name.text).ok_or_else(|| {
            format!(
                "internal error: missing @location assignment for vertex field '{}'",
                field.name.text
            )
        })?;
        layout.push(GpuVertexAttr {
            location,
            format,
            offset,
        });
        offset += vertex_attr_bytes(format);
    }
    Ok(GpuVertexBuffer {
        attributes: layout,
        stride: offset,
        step_mode,
    })
}

fn emit_field_decorators(
    field: &dream_syntax::nodes::struct_node::StructFieldNode,
    locs: &IndexMap<String, u32>,
) -> Result<String, String> {
    let mut parts = String::new();
    if let Some(builtin) = field_builtin(field) {
        parts.push_str(&format!("@builtin({builtin}) "));
    } else {
        let loc = *locs.get(field.name.text.as_str()).ok_or_else(|| {
            format!(
                "internal error: missing @location assignment for field '{}'",
                field.name.text
            )
        })?;
        parts.push_str(&format!("@location({loc}) "));
        if let Some(mode) = field_interpolate_mode(&field.attributes) {
            let Some(interp) = interpolate_wgsl(&mode) else {
                return Err(format!(
                    "unsupported @interpolate({mode:?}); use \"perspective\", \"linear\" or \
                     \"flat\", optionally with a second argument: \"centroid\" or \"sample\" \
                     for the first two, \"first\" or \"either\" for \"flat\""
                ));
            };
            parts.push_str(interp);
            parts.push(' ');
        }
    }
    Ok(parts)
}

/// WGSL name for a stage-interface struct.
///
/// One Dream struct becomes two different WGSL structs across a pipeline: the fragment input
/// form drops the position field, which arrives as the `frag_coord` builtin instead. They need
/// distinct names so the joined sidecar module can declare both.
pub(super) fn interface_struct_wgsl_name(dream_name: &str, for_fragment_input: bool) -> String {
    let base = escape_wgsl_ident(dream_name);
    if for_fragment_input {
        format!("{base}_fs_in")
    } else {
        base
    }
}

pub(super) fn emit_interface_struct_wgsl(
    decl: &StructDeclarationNode<'_>,
    for_fragment_input: bool,
) -> Result<String, String> {
    let locs = assign_locations(decl)?;
    let sname = interface_struct_wgsl_name(&decl.name.text, for_fragment_input);
    let mut s = format!("struct {sname} {{\n");
    for field in &decl.fields {
        let fname = escape_wgsl_ident(&field.name.text);
        let Some(wgsl_ty) = dream_ty_to_wgsl_vec(&field.field_type) else {
            return Err(format!(
                "field '{}' has unsupported shader type '{}'",
                field.name.text,
                field.field_type.get_type()
            ));
        };
        let builtin = field_builtin(field);
        if for_fragment_input && builtin.as_deref() == Some("position") {
            // Fragment inputs get position via the injected `frag_coord` builtin param.
            continue;
        }
        if builtin.as_deref() == Some("position") && wgsl_ty != "vec4<f32>" {
            return Err(format!(
                "builtin position field '{}' must be GpuVec4, found '{}'",
                field.name.text,
                field.field_type.get_type()
            ));
        }
        if builtin.as_deref() == Some("frag_depth") && wgsl_ty != "f32" {
            return Err(format!(
                "builtin frag_depth field '{}' must be float, found '{}'",
                field.name.text,
                field.field_type.get_type()
            ));
        }
        let decorators = emit_field_decorators(field, &locs)?;
        s.push_str(&format!("  {decorators}{fname}: {wgsl_ty},\n"));
    }
    s.push_str("}\n");
    Ok(s)
}

/// A plain data struct, for types passed between helpers rather than across a stage boundary.
///
/// No `@location` or `@builtin` decorators: those describe an interface between pipeline stages,
/// and applying them to an ordinary struct would be rejected by WGSL.
pub(super) fn emit_data_struct_wgsl(decl: &StructDeclarationNode<'_>) -> Result<String, String> {
    let sname = escape_wgsl_ident(&decl.name.text);
    let mut s = format!("struct {sname} {{\n");
    for field in &decl.fields {
        let Some(wgsl_ty) = dream_ty_to_wgsl_vec(&field.field_type) else {
            return Err(format!(
                "field '{}' has unsupported shader type '{}'",
                field.name.text,
                field.field_type.get_type()
            ));
        };
        s.push_str(&format!(
            "  {}: {wgsl_ty},\n",
            escape_wgsl_ident(&field.name.text)
        ));
    }
    s.push_str("}\n");
    Ok(s)
}

/// The one output builtin whose Dream and WGSL types differ, so both the struct emitter and the
/// field-type table have to agree on the exception.
const SAMPLE_MASK_BUILTIN: &str = "sample_mask";

/// Fragment output struct (`@location` color targets + optional `@builtin(frag_depth)` /
/// `@builtin(sample_mask)`).
pub(super) fn emit_fragment_out_struct_wgsl(
    decl: &StructDeclarationNode<'_>,
) -> Result<(String, String), String> {
    let locs = assign_locations(decl)?;
    let sname = escape_wgsl_ident(&decl.name.text);
    let mut s = format!("struct {sname} {{\n");
    let mut has_color = false;
    for field in &decl.fields {
        let fname = escape_wgsl_ident(&field.name.text);
        let Some(wgsl_ty) = dream_ty_to_wgsl_vec(&field.field_type) else {
            return Err(format!(
                "fragment output field '{}' has unsupported type '{}'",
                field.name.text,
                field.field_type.get_type()
            ));
        };
        let builtin = field_builtin(field);
        if let Some(ref b) = builtin {
            match b.as_str() {
                "frag_depth" => {
                    if wgsl_ty != "f32" {
                        return Err(format!(
                            "builtin frag_depth field '{}' must be float",
                            field.name.text
                        ));
                    }
                    s.push_str(&format!("  @builtin(frag_depth) {fname}: {wgsl_ty},\n"));
                }
                // Writing coverage discards samples, which is how alpha-to-coverage and custom
                // MSAA masking work. WGSL types it `u32`; Dream spells it `int` because that is
                // what the bitwise operators produce.
                SAMPLE_MASK_BUILTIN => {
                    if wgsl_ty != "i32" {
                        return Err(format!(
                            "builtin sample_mask field '{}' must be int",
                            field.name.text
                        ));
                    }
                    s.push_str(&format!("  @builtin(sample_mask) {fname}: u32,\n"));
                }
                _ => {
                    return Err(format!(
                        "fragment output field '{}' has unsupported @builtin(\"{b}\"); only \"frag_depth\" and \"sample_mask\" are allowed",
                        field.name.text
                    ));
                }
            }
        } else {
            if wgsl_ty != "vec4<f32>" {
                return Err(format!(
                    "fragment color output '{}' must be GpuVec4, found '{}'",
                    field.name.text,
                    field.field_type.get_type()
                ));
            }
            has_color = true;
            let loc = *locs.get(field.name.text.as_str()).ok_or_else(|| {
                format!(
                    "internal error: missing @location assignment for field '{}'",
                    field.name.text
                )
            })?;
            s.push_str(&format!("  @location({loc}) {fname}: {wgsl_ty},\n"));
        }
    }
    if !has_color {
        return Err(format!(
            "fragment output struct '{}' must have at least one @location color field (GpuVec4)",
            decl.name.text
        ));
    }
    s.push_str("}\n");
    Ok((s, sname))
}

/// Plain value struct for compute storage buffers (no `@location` / `@builtin`).
pub(super) fn emit_value_struct_wgsl(decl: &StructDeclarationNode<'_>) -> Result<String, String> {
    let sname = escape_wgsl_ident(&decl.name.text);
    let mut s = format!("struct {sname} {{\n");
    for field in &decl.fields {
        let fname = escape_wgsl_ident(&field.name.text);
        let Some(wgsl_ty) = dream_ty_to_wgsl_vec(&field.field_type) else {
            return Err(format!(
                "field '{}' has unsupported shader type '{}'",
                field.name.text,
                field.field_type.get_type()
            ));
        };
        s.push_str(&format!("  {fname}: {wgsl_ty},\n"));
    }
    s.push_str("}\n");
    Ok(s)
}

pub(super) fn has_position_gpuvec4(decl: &StructDeclarationNode<'_>) -> bool {
    decl.fields.iter().any(|f| {
        let is_pos = field_is_position_builtin(&f.name.text, &f.attributes);
        is_pos && matches!(&f.field_type, Type::Struct(tok, None) if tok.text == "GpuVec4")
    })
}

/// Count of `@location` color targets in a fragment output struct (excludes builtins).
pub(super) fn fragment_color_target_count(decl: &StructDeclarationNode<'_>) -> Result<u32, String> {
    assign_locations(decl).map(|m| m.len() as u32)
}
