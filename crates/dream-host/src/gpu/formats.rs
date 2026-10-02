//! Wire-code → `wgpu` translation for the format and shape enums in
//! `dream_abi::gpu_format`.
//!
//! The registry there owns the codes and their metadata; this module is only the `wgpu` half of
//! the mapping, kept exhaustive over `gpu_format::ALL` by `every_format_maps` below.

use dream_abi::gpu_format::{
    self, FormatAspect, StorageAccess, TextureFeature, TextureFormatSpec, ViewDimension,
};

/// Everything the host needs to know about a texture's format, resolved once at creation.
#[derive(Clone, Copy)]
pub struct ResolvedFormat {
    pub spec: &'static TextureFormatSpec,
    pub wgpu: wgpu::TextureFormat,
}

impl ResolvedFormat {
    pub fn is_depth(self) -> bool {
        self.spec.aspect != FormatAspect::Color
    }

    /// Bytes per texel for linear CPU copies, or `None` when the format is block-compressed or a
    /// depth format Dream never mirrors on the CPU.
    pub fn bytes_per_texel(self) -> Option<u32> {
        match self.spec.bytes_per_texel {
            0 => None,
            n => Some(n),
        }
    }
}

/// Resolves a `GpuTextureFormat` code, rejecting unknown codes and formats the device lacks the
/// feature for. The feature check happens here so a missing BC/ASTC family surfaces as an
/// `unsupported` error instead of a wgpu validation panic at bind time.
pub fn resolve(code: i32, features: wgpu::Features) -> Result<ResolvedFormat, String> {
    let spec = gpu_format::by_code(code).ok_or_else(|| format!("unknown texture format {code}"))?;
    if let Some(required) = spec.feature {
        let flag = feature_flag(required);
        // `float32-filterable` only upgrades sampling; the format itself is always creatable.
        if required != TextureFeature::Float32Filterable && !features.contains(flag) {
            return Err(format!(
                "unsupported: texture format {} needs device feature {flag:?}",
                spec.name
            ));
        }
    }
    Ok(ResolvedFormat {
        spec,
        wgpu: to_wgpu(spec),
    })
}

fn feature_flag(feature: TextureFeature) -> wgpu::Features {
    match feature {
        TextureFeature::BcCompression => wgpu::Features::TEXTURE_COMPRESSION_BC,
        TextureFeature::Etc2Compression => wgpu::Features::TEXTURE_COMPRESSION_ETC2,
        TextureFeature::AstcCompression => wgpu::Features::TEXTURE_COMPRESSION_ASTC,
        TextureFeature::Depth32FloatStencil8 => wgpu::Features::DEPTH32FLOAT_STENCIL8,
        TextureFeature::Float32Filterable => wgpu::Features::FLOAT32_FILTERABLE,
    }
}

fn to_wgpu(spec: &TextureFormatSpec) -> wgpu::TextureFormat {
    use wgpu::TextureFormat as F;
    match spec.code {
        0 => F::R8Unorm,
        1 => F::Rg8Unorm,
        2 => F::Rgba8Unorm,
        3 => F::Rgba8UnormSrgb,
        4 => F::Bgra8Unorm,
        5 => F::Bgra8UnormSrgb,
        6 => F::R16Float,
        7 => F::Rg16Float,
        8 => F::Rgba16Float,
        9 => F::R32Float,
        10 => F::Rg32Float,
        11 => F::Rgba32Float,
        12 => F::Rg11b10Ufloat,
        13 => F::Rgb10a2Unorm,
        14 => F::Depth16Unorm,
        15 => F::Depth24Plus,
        16 => F::Depth24PlusStencil8,
        17 => F::Depth32Float,
        18 => F::Depth32FloatStencil8,
        19 => F::Bc1RgbaUnorm,
        20 => F::Bc3RgbaUnorm,
        21 => F::Bc5RgUnorm,
        22 => F::Bc7RgbaUnorm,
        23 => F::Etc2Rgb8Unorm,
        24 => F::Etc2Rgba8Unorm,
        25 => F::Astc {
            block: wgpu::AstcBlock::B4x4,
            channel: wgpu::AstcChannel::Unorm,
        },
        26 => F::Astc {
            block: wgpu::AstcBlock::B8x8,
            channel: wgpu::AstcChannel::Unorm,
        },
        // Unreachable: `resolve` only produces specs from `gpu_format::ALL`, and
        // `every_format_maps` asserts this match covers all of them.
        other => unreachable!("texture format code {} has no wgpu mapping", other),
    }
}

/// The `wgpu` format code for a `GpuTextureFormat`, without a feature check. Used where the format
/// came from an already-created texture.
pub fn to_wgpu_unchecked(spec: &TextureFormatSpec) -> wgpu::TextureFormat {
    to_wgpu(spec)
}

pub fn dimension(code: i32) -> Result<wgpu::TextureDimension, String> {
    Ok(match code {
        gpu_format::DIM_1D => wgpu::TextureDimension::D1,
        gpu_format::DIM_2D => wgpu::TextureDimension::D2,
        gpu_format::DIM_3D => wgpu::TextureDimension::D3,
        other => return Err(format!("unknown texture dimension {other}")),
    })
}

pub fn view_dimension(code: i32) -> Result<wgpu::TextureViewDimension, String> {
    let dim =
        ViewDimension::from_code(code).ok_or_else(|| format!("unknown view dimension {code}"))?;
    Ok(to_wgpu_view(dim))
}

pub fn to_wgpu_view(dim: ViewDimension) -> wgpu::TextureViewDimension {
    match dim {
        ViewDimension::D1 => wgpu::TextureViewDimension::D1,
        ViewDimension::D2 => wgpu::TextureViewDimension::D2,
        ViewDimension::D2Array => wgpu::TextureViewDimension::D2Array,
        ViewDimension::Cube => wgpu::TextureViewDimension::Cube,
        ViewDimension::CubeArray => wgpu::TextureViewDimension::CubeArray,
        ViewDimension::D3 => wgpu::TextureViewDimension::D3,
    }
}

pub fn storage_access(code: i32) -> Result<wgpu::StorageTextureAccess, String> {
    let access =
        StorageAccess::from_code(code).ok_or_else(|| format!("unknown storage access {code}"))?;
    Ok(match access {
        StorageAccess::ReadOnly => wgpu::StorageTextureAccess::ReadOnly,
        StorageAccess::WriteOnly => wgpu::StorageTextureAccess::WriteOnly,
        StorageAccess::ReadWrite => wgpu::StorageTextureAccess::ReadWrite,
    })
}

/// `GpuCompareFunction` code → `wgpu`. Shared by comparison samplers and depth state.
pub fn compare_function(code: i32) -> Result<wgpu::CompareFunction, String> {
    Ok(match code {
        0 => wgpu::CompareFunction::Never,
        1 => wgpu::CompareFunction::Less,
        2 => wgpu::CompareFunction::Equal,
        3 => wgpu::CompareFunction::LessEqual,
        4 => wgpu::CompareFunction::Greater,
        5 => wgpu::CompareFunction::NotEqual,
        6 => wgpu::CompareFunction::GreaterEqual,
        7 => wgpu::CompareFunction::Always,
        other => return Err(format!("unknown compare function {other}")),
    })
}

pub fn filter_mode(code: i32) -> wgpu::FilterMode {
    if code == 1 {
        wgpu::FilterMode::Linear
    } else {
        wgpu::FilterMode::Nearest
    }
}

pub fn address_mode(code: i32) -> wgpu::AddressMode {
    match code {
        1 => wgpu::AddressMode::Repeat,
        2 => wgpu::AddressMode::MirrorRepeat,
        _ => wgpu::AddressMode::ClampToEdge,
    }
}

/// The `.abi.json` vertex layout names its attribute formats, so resolution goes through the
/// shared registry by spelling — the stride the host computes and the one the emitter reports
/// come from the same entry and cannot drift.
pub fn vertex_format_named(name: &str) -> Option<(wgpu::VertexFormat, u32)> {
    let spec = gpu_format::vertex_format_by_name(name)?;
    Some((vertex_format_spec(spec), spec.size))
}

fn vertex_format_spec(spec: &gpu_format::VertexFormatSpec) -> wgpu::VertexFormat {
    use wgpu::VertexFormat as V;
    match spec.code {
        0 => V::Uint8x2,
        1 => V::Uint8x4,
        2 => V::Sint8x2,
        3 => V::Sint8x4,
        4 => V::Unorm8x2,
        5 => V::Unorm8x4,
        6 => V::Snorm8x2,
        7 => V::Snorm8x4,
        8 => V::Uint16x2,
        9 => V::Uint16x4,
        10 => V::Sint16x2,
        11 => V::Sint16x4,
        12 => V::Unorm16x2,
        13 => V::Unorm16x4,
        14 => V::Snorm16x2,
        15 => V::Snorm16x4,
        16 => V::Float16x2,
        17 => V::Float16x4,
        18 => V::Float32,
        19 => V::Float32x2,
        20 => V::Float32x3,
        21 => V::Float32x4,
        22 => V::Uint32,
        23 => V::Uint32x2,
        24 => V::Uint32x3,
        25 => V::Uint32x4,
        26 => V::Sint32,
        27 => V::Sint32x2,
        28 => V::Sint32x3,
        29 => V::Sint32x4,
        other => unreachable!("vertex format code {} has no wgpu mapping", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `wgpu::TextureFormat` has no `Display`, so the mapping is checked against the properties
    /// the registry claims rather than against the name.
    #[test]
    fn every_format_maps() {
        let mut seen = Vec::new();
        for spec in gpu_format::ALL {
            // Panics via `unreachable!` if the match above is missing a code.
            let f = to_wgpu(spec);
            assert!(!seen.contains(&f), "{} duplicates another format", spec.variant);
            seen.push(f);

            assert_eq!(
                f.has_depth_aspect(),
                spec.aspect != FormatAspect::Color,
                "{} depth aspect",
                spec.variant
            );
            assert_eq!(
                f.has_stencil_aspect(),
                spec.aspect == FormatAspect::DepthStencil,
                "{} stencil aspect",
                spec.variant
            );
            match spec.block {
                Some((w, h)) => assert_eq!(f.block_dimensions(), (w, h), "{} block", spec.variant),
                None => assert_eq!(f.block_dimensions(), (1, 1), "{} block", spec.variant),
            }
            if let Some(bpt) = (ResolvedFormat { spec, wgpu: f }).bytes_per_texel() {
                assert_eq!(
                    f.block_copy_size(None),
                    Some(bpt),
                    "{} bytes per texel",
                    spec.variant
                );
            }
        }
    }

    #[test]
    fn every_vertex_format_maps() {
        for spec in gpu_format::VERTEX_FORMATS {
            let (vf, size) = vertex_format_named(spec.name).expect("mapped");
            assert_eq!(size, spec.size, "{} stride", spec.variant);
            assert_eq!(vf.size() as u32, spec.size, "{} wgpu stride", spec.variant);
        }
    }
}
