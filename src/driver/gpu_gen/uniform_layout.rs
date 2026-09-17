//! Size of a shader's uniform block under WGSL's uniform address space layout rules.
//!
//! The host binds the uniform block as a fixed-size window into a ring buffer, so it needs the
//! block's exact size before any draw happens. WGSL pads `vec3` to 16 bytes, aligns matrix columns
//! individually, and rounds a struct in the uniform address space up to a multiple of 16 — so the
//! size is not the sum of the field sizes, and `Uniforms.pack` on the Dream side has to agree with
//! it byte for byte.

/// Alignment and size of one WGSL type, as the uniform address space sees it.
fn align_size(wgsl: &str) -> Option<(u32, u32)> {
    Some(match wgsl {
        "f32" | "i32" | "u32" => (4, 4),
        "vec2<f32>" | "vec2<i32>" | "vec2<u32>" => (8, 8),
        // A `vec3` occupies 12 bytes but aligns to 16, so a following scalar packs into the gap.
        "vec3<f32>" | "vec3<i32>" | "vec3<u32>" => (16, 12),
        "vec4<f32>" | "vec4<i32>" | "vec4<u32>" => (16, 16),
        "mat2x2<f32>" => (8, 16),
        "mat3x3<f32>" => (16, 48),
        "mat4x4<f32>" => (16, 64),
        _ => return None,
    })
}

fn round_up(n: u32, align: u32) -> u32 {
    n.div_ceil(align) * align
}

/// Byte size of a uniform block holding `fields` in declaration order.
///
/// `Err` names the first field whose type has no uniform-space layout, which is the same set the
/// WGSL emitter would reject anyway.
pub(super) fn block_size(fields: &[(String, String)]) -> Result<u32, String> {
    let mut offset = 0u32;
    let mut max_align = 16u32; // Structs in the uniform address space align to at least 16.
    for (name, ty) in fields {
        let (align, size) = align_size(ty).ok_or_else(|| {
            format!("uniform parameter '{name}' has type '{ty}', which has no uniform block layout")
        })?;
        max_align = max_align.max(align);
        offset = round_up(offset, align) + size;
    }
    Ok(round_up(offset, max_align))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(fields: &[(&str, &str)]) -> u32 {
        let owned: Vec<(String, String)> = fields
            .iter()
            .map(|(n, t)| ((*n).to_string(), (*t).to_string()))
            .collect();
        block_size(&owned).expect("laid out")
    }

    #[test]
    fn rounds_block_up_to_sixteen() {
        assert_eq!(size(&[("a", "f32")]), 16);
        assert_eq!(size(&[("a", "f32"), ("b", "f32")]), 16);
        assert_eq!(size(&[("a", "vec4<f32>")]), 16);
    }

    #[test]
    fn vec3_leaves_a_gap_a_scalar_fills() {
        // vec3 ends at 12 and f32 aligns to 4, so both fit in one 16-byte stride.
        assert_eq!(size(&[("a", "vec3<f32>"), ("b", "f32")]), 16);
        // A vec2 cannot start at 12, so it pads to 16 and the block grows to 32.
        assert_eq!(size(&[("a", "vec3<f32>"), ("b", "vec2<f32>")]), 32);
    }

    #[test]
    fn matrices_keep_column_alignment() {
        assert_eq!(size(&[("m", "mat4x4<f32>")]), 64);
        assert_eq!(size(&[("m", "mat3x3<f32>")]), 48);
        assert_eq!(size(&[("m", "mat2x2<f32>")]), 16);
        assert_eq!(size(&[("m", "mat4x4<f32>"), ("t", "f32")]), 80);
    }

    #[test]
    fn scalar_before_vec4_pads_to_the_vector() {
        assert_eq!(size(&[("a", "f32"), ("b", "vec4<f32>")]), 32);
    }

    #[test]
    fn unsupported_type_names_the_field() {
        let err = block_size(&[("bad".into(), "array<f32>".into())]).expect_err("rejected");
        assert!(err.contains("'bad'"), "{}", err);
    }
}
