//! Texture format and shape registry shared by the WGSL emitter and both GPU hosts.
//!
//! The format *code* is the wire value: Dream's `GpuTextureFormat` enum discriminant, passed to the
//! hosts as an `int`. This table is the single source of truth for what each code means. Codes are
//! append-only — inserting into the middle silently reinterprets every `.dream` file and both
//! hosts.
//!
//! Three places must agree, and two of them are checked:
//! - `crates/dream-stdlib/src/system/gpu/gpu_enums.dream` (`GpuTextureFormat`) — asserted against
//!   this table by `gpu_format_table_matches_stdlib_enum` in the root crate.
//! - `src/execution/host/gpu/formats.rs` maps codes to `wgpu::TextureFormat` and is exhaustive over
//!   [`ALL`].
//! - `runtime/src/hosts/gpu.js` (`TEXTURE_FORMATS`) mirrors the WebGPU names by code; the JS side
//!   cannot be checked from Rust, so it carries a pointer back here.

use std::convert::TryFrom;

/// What a format can be attached as, which decides its default usage flags and whether an RGBA
/// read-back or write is meaningful at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatAspect {
    Color,
    Depth,
    DepthStencil,
}

/// The `BindGroupLayoutEntry` sample type a sampled binding of this format needs. `rgba32float` and
/// friends are unfilterable without a device feature, and depth formats bind as `Depth`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleType {
    Float,
    UnfilterableFloat,
    Depth,
}

#[derive(Debug, Clone, Copy)]
pub struct TextureFormatSpec {
    /// `GpuTextureFormat` discriminant.
    pub code: i32,
    /// `GpuTextureFormat` variant name, for the stdlib cross-check.
    pub variant: &'static str,
    /// WebGPU / WGSL spelling. `wgpu`, WGSL storage-texture types, and the JS host all use this.
    pub name: &'static str,
    pub aspect: FormatAspect,
    pub sample_type: SampleType,
    /// Bytes per texel, or `0` for block-compressed and depth formats that Dream never
    /// linearly copies.
    pub bytes_per_texel: u32,
    /// Block footprint for compressed formats, `None` when a texel is a block of 1x1.
    pub block: Option<(u32, u32)>,
    /// Usable as a WGSL `texture_storage_*` binding. WebGPU only guarantees this for a fixed set.
    pub storage: bool,
    /// Needs a device feature beyond the guaranteed baseline, so creation is rejected when the
    /// adapter lacks it rather than failing deep inside wgpu.
    pub feature: Option<TextureFeature>,
}

/// Optional device features a format can require.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFeature {
    BcCompression,
    Etc2Compression,
    AstcCompression,
    Depth32FloatStencil8,
    Float32Filterable,
}

macro_rules! fmt {
    (
        $code:expr, $variant:literal, $name:literal, $aspect:ident, $sample:ident,
        $bpt:expr, $block:expr, $storage:expr, $feature:expr
    ) => {
        TextureFormatSpec {
            code: $code,
            variant: $variant,
            name: $name,
            aspect: FormatAspect::$aspect,
            sample_type: SampleType::$sample,
            bytes_per_texel: $bpt,
            block: $block,
            storage: $storage,
            feature: $feature,
        }
    };
}

/// Every format Dream can create, indexed by code (`ALL[i].code == i`).
pub const ALL: &[TextureFormatSpec] = &[
    fmt!(0, "R8Unorm", "r8unorm", Color, Float, 1, None, false, None),
    fmt!(1, "Rg8Unorm", "rg8unorm", Color, Float, 2, None, false, None),
    fmt!(2, "Rgba8Unorm", "rgba8unorm", Color, Float, 4, None, true, None),
    // sRGB and BGRA cannot be storage bindings even though their linear/swapped twins can.
    fmt!(
        3,
        "Rgba8UnormSrgb",
        "rgba8unorm-srgb",
        Color,
        Float,
        4,
        None,
        false,
        None
    ),
    fmt!(4, "Bgra8Unorm", "bgra8unorm", Color, Float, 4, None, false, None),
    fmt!(
        5,
        "Bgra8UnormSrgb",
        "bgra8unorm-srgb",
        Color,
        Float,
        4,
        None,
        false,
        None
    ),
    fmt!(6, "R16Float", "r16float", Color, Float, 2, None, true, None),
    fmt!(7, "Rg16Float", "rg16float", Color, Float, 4, None, true, None),
    fmt!(8, "Rgba16Float", "rgba16float", Color, Float, 8, None, true, None),
    fmt!(
        9,
        "R32Float",
        "r32float",
        Color,
        UnfilterableFloat,
        4,
        None,
        true,
        Some(TextureFeature::Float32Filterable)
    ),
    fmt!(
        10,
        "Rg32Float",
        "rg32float",
        Color,
        UnfilterableFloat,
        8,
        None,
        true,
        Some(TextureFeature::Float32Filterable)
    ),
    fmt!(
        11,
        "Rgba32Float",
        "rgba32float",
        Color,
        UnfilterableFloat,
        16,
        None,
        true,
        Some(TextureFeature::Float32Filterable)
    ),
    fmt!(
        12,
        "Rg11b10Ufloat",
        "rg11b10ufloat",
        Color,
        Float,
        4,
        None,
        false,
        None
    ),
    fmt!(
        13,
        "Rgb10a2Unorm",
        "rgb10a2unorm",
        Color,
        Float,
        4,
        None,
        false,
        None
    ),
    fmt!(14, "Depth16Unorm", "depth16unorm", Depth, Depth, 0, None, false, None),
    fmt!(15, "Depth24Plus", "depth24plus", Depth, Depth, 0, None, false, None),
    fmt!(
        16,
        "Depth24PlusStencil8",
        "depth24plus-stencil8",
        DepthStencil,
        Depth,
        0,
        None,
        false,
        None
    ),
    fmt!(
        17,
        "Depth32Float",
        "depth32float",
        Depth,
        Depth,
        0,
        None,
        false,
        None
    ),
    fmt!(
        18,
        "Depth32FloatStencil8",
        "depth32float-stencil8",
        DepthStencil,
        Depth,
        0,
        None,
        false,
        Some(TextureFeature::Depth32FloatStencil8)
    ),
    fmt!(
        19,
        "Bc1RgbaUnorm",
        "bc1-rgba-unorm",
        Color,
        Float,
        0,
        Some((4, 4)),
        false,
        Some(TextureFeature::BcCompression)
    ),
    fmt!(
        20,
        "Bc3RgbaUnorm",
        "bc3-rgba-unorm",
        Color,
        Float,
        0,
        Some((4, 4)),
        false,
        Some(TextureFeature::BcCompression)
    ),
    fmt!(
        21,
        "Bc5RgUnorm",
        "bc5-rg-unorm",
        Color,
        Float,
        0,
        Some((4, 4)),
        false,
        Some(TextureFeature::BcCompression)
    ),
    fmt!(
        22,
        "Bc7RgbaUnorm",
        "bc7-rgba-unorm",
        Color,
        Float,
        0,
        Some((4, 4)),
        false,
        Some(TextureFeature::BcCompression)
    ),
    fmt!(
        23,
        "Etc2Rgb8Unorm",
        "etc2-rgb8unorm",
        Color,
        Float,
        0,
        Some((4, 4)),
        false,
        Some(TextureFeature::Etc2Compression)
    ),
    fmt!(
        24,
        "Etc2Rgba8Unorm",
        "etc2-rgba8unorm",
        Color,
        Float,
        0,
        Some((4, 4)),
        false,
        Some(TextureFeature::Etc2Compression)
    ),
    fmt!(
        25,
        "Astc4x4Unorm",
        "astc-4x4-unorm",
        Color,
        Float,
        0,
        Some((4, 4)),
        false,
        Some(TextureFeature::AstcCompression)
    ),
    fmt!(
        26,
        "Astc8x8Unorm",
        "astc-8x8-unorm",
        Color,
        Float,
        0,
        Some((8, 8)),
        false,
        Some(TextureFeature::AstcCompression)
    ),
];

/// The spec for `code`, or `None` when a guest passed a value this compiler does not know.
pub fn by_code(code: i32) -> Option<&'static TextureFormatSpec> {
    ALL.get(usize::try_from(code).ok()?)
}

pub fn by_name(name: &str) -> Option<&'static TextureFormatSpec> {
    ALL.iter().find(|f| f.name == name)
}

/// Texture shape, matching Dream's `GpuTextureDimension` and WebGPU's `GPUTextureDimension`.
pub const DIM_1D: i32 = 0;
pub const DIM_2D: i32 = 1;
pub const DIM_3D: i32 = 2;

/// How a binding views a texture, matching Dream's `GpuTextureViewDimension`. The emitter picks
/// the WGSL type from this, so the names double as the `texture_*` suffix table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewDimension {
    D1,
    D2,
    D2Array,
    Cube,
    CubeArray,
    D3,
}

impl ViewDimension {
    pub fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => Self::D1,
            1 => Self::D2,
            2 => Self::D2Array,
            3 => Self::Cube,
            4 => Self::CubeArray,
            5 => Self::D3,
            _ => return None,
        })
    }

    /// WebGPU spelling, used by the JS host and `wgpu`'s serde names.
    pub fn name(self) -> &'static str {
        match self {
            Self::D1 => "1d",
            Self::D2 => "2d",
            Self::D2Array => "2d-array",
            Self::Cube => "cube",
            Self::CubeArray => "cube-array",
            Self::D3 => "3d",
        }
    }

    /// WGSL `texture_*` / `texture_storage_*` suffix for this view.
    pub fn wgsl_suffix(self) -> &'static str {
        match self {
            Self::D1 => "1d",
            Self::D2 => "2d",
            Self::D2Array => "2d_array",
            Self::Cube => "cube",
            Self::CubeArray => "cube_array",
            Self::D3 => "3d",
        }
    }
}

/// Storage-texture access mode, matching Dream's `GpuStorageAccess` and WGSL's access qualifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageAccess {
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

impl StorageAccess {
    pub fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => Self::ReadOnly,
            1 => Self::WriteOnly,
            2 => Self::ReadWrite,
            _ => return None,
        })
    }

    /// WGSL access qualifier.
    pub fn wgsl(self) -> &'static str {
        match self {
            Self::ReadOnly => "read",
            Self::WriteOnly => "write",
            Self::ReadWrite => "read_write",
        }
    }

    /// WebGPU `GPUStorageTextureAccess` spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::WriteOnly => "write-only",
            Self::ReadWrite => "read-write",
        }
    }
}

/// A single vertex attribute format. `code` is Dream's `GpuVertexFormat` discriminant.
#[derive(Debug, Clone, Copy)]
pub struct VertexFormatSpec {
    pub code: i32,
    /// `GpuVertexFormat` variant name, for the stdlib cross-check.
    pub variant: &'static str,
    /// WebGPU / `wgpu` spelling.
    pub name: &'static str,
    /// Size in bytes, needed to validate strides and to derive a default packed layout.
    pub size: u32,
    /// WGSL type this attribute must be declared as in the vertex entry point.
    pub wgsl: &'static str,
}

macro_rules! vfmt {
    ($code:expr, $variant:literal, $name:literal, $size:expr, $wgsl:literal) => {
        VertexFormatSpec {
            code: $code,
            variant: $variant,
            name: $name,
            size: $size,
            wgsl: $wgsl,
        }
    };
}

/// Every vertex attribute format Dream can declare, indexed by code (`VERTEX_FORMATS[i].code == i`).
pub const VERTEX_FORMATS: &[VertexFormatSpec] = &[
    vfmt!(0, "Uint8x2", "uint8x2", 2, "vec2<u32>"),
    vfmt!(1, "Uint8x4", "uint8x4", 4, "vec4<u32>"),
    vfmt!(2, "Sint8x2", "sint8x2", 2, "vec2<i32>"),
    vfmt!(3, "Sint8x4", "sint8x4", 4, "vec4<i32>"),
    vfmt!(4, "Unorm8x2", "unorm8x2", 2, "vec2<f32>"),
    vfmt!(5, "Unorm8x4", "unorm8x4", 4, "vec4<f32>"),
    vfmt!(6, "Snorm8x2", "snorm8x2", 2, "vec2<f32>"),
    vfmt!(7, "Snorm8x4", "snorm8x4", 4, "vec4<f32>"),
    vfmt!(8, "Uint16x2", "uint16x2", 4, "vec2<u32>"),
    vfmt!(9, "Uint16x4", "uint16x4", 8, "vec4<u32>"),
    vfmt!(10, "Sint16x2", "sint16x2", 4, "vec2<i32>"),
    vfmt!(11, "Sint16x4", "sint16x4", 8, "vec4<i32>"),
    vfmt!(12, "Unorm16x2", "unorm16x2", 4, "vec2<f32>"),
    vfmt!(13, "Unorm16x4", "unorm16x4", 8, "vec4<f32>"),
    vfmt!(14, "Snorm16x2", "snorm16x2", 4, "vec2<f32>"),
    vfmt!(15, "Snorm16x4", "snorm16x4", 8, "vec4<f32>"),
    vfmt!(16, "Float16x2", "float16x2", 4, "vec2<f32>"),
    vfmt!(17, "Float16x4", "float16x4", 8, "vec4<f32>"),
    vfmt!(18, "Float32", "float32", 4, "f32"),
    vfmt!(19, "Float32x2", "float32x2", 8, "vec2<f32>"),
    vfmt!(20, "Float32x3", "float32x3", 12, "vec3<f32>"),
    vfmt!(21, "Float32x4", "float32x4", 16, "vec4<f32>"),
    vfmt!(22, "Uint32", "uint32", 4, "u32"),
    vfmt!(23, "Uint32x2", "uint32x2", 8, "vec2<u32>"),
    vfmt!(24, "Uint32x3", "uint32x3", 12, "vec3<u32>"),
    vfmt!(25, "Uint32x4", "uint32x4", 16, "vec4<u32>"),
    vfmt!(26, "Sint32", "sint32", 4, "i32"),
    vfmt!(27, "Sint32x2", "sint32x2", 8, "vec2<i32>"),
    vfmt!(28, "Sint32x3", "sint32x3", 12, "vec3<i32>"),
    vfmt!(29, "Sint32x4", "sint32x4", 16, "vec4<i32>"),
];

pub fn vertex_format_by_code(code: i32) -> Option<&'static VertexFormatSpec> {
    VERTEX_FORMATS.get(usize::try_from(code).ok()?)
}

/// Lookup by WebGPU spelling, the form the `.abi.json` vertex layout carries.
pub fn vertex_format_by_name(name: &str) -> Option<&'static VertexFormatSpec> {
    VERTEX_FORMATS.iter().find(|f| f.name == name)
}

/// `GpuVertexStepMode` wire values.
pub const STEP_MODE_VERTEX: i32 = 0;
pub const STEP_MODE_INSTANCE: i32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_codes_are_dense_and_sized() {
        for (i, f) in VERTEX_FORMATS.iter().enumerate() {
            assert_eq!(f.code, i as i32, "{} is out of order", f.variant);
            assert!(f.size > 0 && f.size % 2 == 0, "{} has a bad size", f.variant);
        }
    }

    #[test]
    fn codes_are_dense_and_in_order() {
        for (i, f) in ALL.iter().enumerate() {
            assert_eq!(f.code, i as i32, "{} is out of order", f.variant);
        }
    }

    #[test]
    fn names_are_unique() {
        let mut names: Vec<&str> = ALL.iter().map(|f| f.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate format name");
    }

    #[test]
    fn depth_formats_are_never_storage_or_compressed() {
        for f in ALL.iter().filter(|f| f.aspect != FormatAspect::Color) {
            assert!(!f.storage, "{} cannot be a storage texture", f.variant);
            assert!(f.block.is_none(), "{} is not block compressed", f.variant);
            assert_eq!(f.sample_type, SampleType::Depth);
        }
    }

    #[test]
    fn compressed_formats_have_a_block_and_a_feature() {
        for f in ALL.iter().filter(|f| f.block.is_some()) {
            assert_eq!(f.bytes_per_texel, 0, "{} is not linearly copyable", f.variant);
            assert!(f.feature.is_some(), "{} needs a device feature", f.variant);
        }
    }
}
