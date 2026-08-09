//! Vertex attribute / varying location assignment and WGSL type helpers for render structs.

use dream_abi::attributes::field_location_override;
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::types::Type;
use dream_syntax::nodes::ProgramNode;
use indexmap::IndexMap;

use super::types::GpuVertexAttr;

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
            _ => None,
        },
        _ => None,
    }
}

/// Struct → field → WGSL type map for member-access inference in shaders.
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

    for st in &program.structs {
        let mut fields = IndexMap::new();
        for field in &st.fields {
            let ty = dream_ty_to_wgsl_vec(&field.field_type)
                .map(|s| s.to_string())
                .unwrap_or_else(|| dream_ty_to_wgsl(&field.field_type));
            fields.insert(field.name.text.clone(), ty);
        }
        map.insert(st.name.text.clone(), fields);
    }
    map
}

pub(super) fn vertex_format_of(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Float(_) | Type::Double(_) => Some("float32"),
        Type::Struct(tok, None) => match tok.text.as_str() {
            "GpuVec2" => Some("float32x2"),
            "GpuVec3" => Some("float32x3"),
            "GpuVec4" => Some("float32x4"),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn vertex_attr_bytes(format: &str) -> u32 {
    match format {
        "float32" => 4,
        "float32x2" => 8,
        "float32x3" => 12,
        "float32x4" => 16,
        _ => 4,
    }
}

/// Assign locations by declaration order, honoring optional `@location(N)` overrides.
/// Skips a field named `position` when `skip_position` is true (VS out / FS in).
pub(super) fn assign_locations(
    decl: &StructDeclarationNode<'_>,
    skip_position: bool,
) -> Result<IndexMap<String, u32>, String> {
    let mut map = IndexMap::new();
    let mut used: IndexMap<u32, String> = IndexMap::new();
    let mut auto = 0u32;
    for field in &decl.fields {
        if skip_position && field.name.text == "position" {
            continue;
        }
        let loc = match field_location_override(&field.attributes) {
            Some(n) => n,
            None => {
                while used.contains_key(&auto) {
                    auto += 1;
                }
                auto
            }
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

pub(super) fn build_vertex_layout(
    decl: &StructDeclarationNode<'_>,
) -> Result<(Vec<GpuVertexAttr>, u32), String> {
    let locs = assign_locations(decl, false)?;
    let mut layout = Vec::new();
    let mut offset = 0u32;
    for field in &decl.fields {
        let Some(format) = vertex_format_of(&field.field_type) else {
            return Err(format!(
                "vertex attribute '{}' has unsupported type '{}'; use float or GpuVec2/3/4",
                field.name.text,
                field.field_type.get_type()
            ));
        };
        let location = *locs.get(&field.name.text).unwrap_or(&0);
        layout.push(GpuVertexAttr {
            location,
            format,
            offset,
        });
        offset += vertex_attr_bytes(format);
    }
    Ok((layout, offset))
}

pub(super) fn emit_interface_struct_wgsl(
    decl: &StructDeclarationNode<'_>,
    for_fragment_input: bool,
) -> Result<String, String> {
    let locs = assign_locations(decl, true)?;
    let mut s = format!("struct {} {{\n", decl.name.text);
    for field in &decl.fields {
        let fname = &field.name.text;
        let Some(wgsl_ty) = dream_ty_to_wgsl_vec(&field.field_type) else {
            return Err(format!(
                "field '{fname}' has unsupported shader type '{}'",
                field.field_type.get_type()
            ));
        };
        if fname == "position" {
            if for_fragment_input {
                continue;
            }
            if wgsl_ty != "vec4<f32>" {
                return Err(format!(
                    "field 'position' must be GpuVec4, found '{}'",
                    field.field_type.get_type()
                ));
            }
            s.push_str(&format!("  @builtin(position) {fname}: {wgsl_ty},\n"));
        } else {
            let loc = locs.get(fname).copied().unwrap_or(0);
            s.push_str(&format!("  @location({loc}) {fname}: {wgsl_ty},\n"));
        }
    }
    s.push_str("}\n");
    Ok(s)
}

pub(super) fn has_position_gpuvec4(decl: &StructDeclarationNode<'_>) -> bool {
    decl.fields.iter().any(|f| {
        f.name.text == "position"
            && matches!(&f.field_type, Type::Struct(tok, None) if tok.text == "GpuVec4")
    })
}
