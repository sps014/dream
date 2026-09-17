//! Public GPU shader metadata types.

/// One storage/uniform/texture/sampler binding derived from a shader parameter.
#[derive(Debug, Clone)]
pub struct GpuBinding {
    pub name: String,
    /// WGSL `@group(N)`. Binding indices are unique per group, not per shader.
    pub group: u32,
    pub binding: u32,
    /// `"storage"`, `"uniform"`, `"texture"`, `"storage_texture"`, or `"sampler"`.
    pub kind: &'static str,
    /// WGSL element / scalar / texture type (`f32`, `i32`, `texture_2d<f32>`, …).
    pub wgsl_ty: String,
    pub read_write: bool,
    /// When true, storage element type is `atomic<…>` (int/uint buffers used with atomics).
    pub atomic: bool,
    /// Texture bindings: WebGPU `viewDimension` (`"2d"`, `"cube"`, …). Empty for non-textures.
    pub view_dimension: &'static str,
    /// Sampled textures: WebGPU `sampleType` (`"float"`, `"unfilterable-float"`, `"depth"`).
    /// Samplers: the binding type (`"filtering"`, `"comparison"`). Empty otherwise.
    pub sample_type: &'static str,
    pub multisampled: bool,
    /// Storage textures: texel format and access, both empty otherwise.
    pub storage_format: &'static str,
    pub storage_access: &'static str,
}

impl GpuBinding {
    /// A non-texture, non-sampler binding: buffers and the uniform block.
    pub fn buffer(
        name: String,
        group: u32,
        binding: u32,
        kind: &'static str,
        wgsl_ty: String,
        read_write: bool,
        atomic: bool,
    ) -> Self {
        Self {
            name,
            group,
            binding,
            kind,
            wgsl_ty,
            read_write,
            atomic,
            view_dimension: "",
            sample_type: "",
            multisampled: false,
            storage_format: "",
            storage_access: "",
        }
    }
}

/// One vertex-buffer attribute slot.
#[derive(Debug, Clone)]
pub struct GpuVertexAttr {
    pub location: u32,
    /// WebGPU vertex format name; the wire format, which `@format` may pack below the field type.
    pub format: &'static str,
    pub offset: u32,
}

/// One vertex buffer slot: the struct a `@vertex` parameter takes, as a buffer layout.
#[derive(Debug, Clone)]
pub struct GpuVertexBuffer {
    pub attributes: Vec<GpuVertexAttr>,
    pub stride: u32,
    /// `"vertex"` or `"instance"` (`@instance` on the parameter).
    pub step_mode: &'static str,
}

/// Metadata for one `@compute` kernel.
#[derive(Debug, Clone)]
pub struct GpuKernelInfo {
    pub name: String,
    pub entry: String,
    pub workgroup: (u32, u32, u32),
    pub bindings: Vec<GpuBinding>,
    /// Byte size of the shared uniform block, `0` when the kernel declares none. The host binds
    /// the block as a window of exactly this size into its uniform ring.
    pub uniform_size: u32,
    pub wgsl: String,
}

/// Metadata for one `@vertex` or `@fragment` shader.
#[derive(Debug, Clone)]
pub struct GpuShaderInfo {
    pub name: String,
    /// `"vertex"` or `"fragment"`.
    pub stage: &'static str,
    pub entry: String,
    pub bindings: Vec<GpuBinding>,
    /// Vertex stage only: one layout per vertex-struct parameter, in `set_vertex_buffer` slot
    /// order.
    pub vertex_buffers: Vec<GpuVertexBuffer>,
    /// Dream type name of the VS return / FS first param interface struct (empty if none).
    pub interface_ty: String,
    /// Fragment stage: number of `@location` color targets (1 for bare `GpuVec4` return).
    pub color_targets: u32,
    /// Byte size of the shared uniform block, `0` when the stage declares none. The host binds
    /// the block as a window of exactly this size into its uniform ring.
    pub uniform_size: u32,
    pub wgsl: String,
}

/// Combined GPU emit result for ABI / sidecar.
#[derive(Debug, Clone, Default)]
pub struct GpuEmitResult {
    pub kernels: Vec<GpuKernelInfo>,
    pub shaders: Vec<GpuShaderInfo>,
}

impl GpuEmitResult {
    pub fn is_empty(&self) -> bool {
        self.kernels.is_empty() && self.shaders.is_empty()
    }
}
