//! Public GPU shader metadata types.

/// One storage/uniform/texture/sampler binding derived from a shader parameter.
#[derive(Debug, Clone)]
pub struct GpuBinding {
    pub name: String,
    pub binding: u32,
    /// `"storage"`, `"uniform"`, `"texture"`, `"storage_texture"`, or `"sampler"`.
    pub kind: &'static str,
    /// WGSL element / scalar / texture type (`f32`, `i32`, `texture_2d<f32>`, …).
    pub wgsl_ty: String,
    pub read_write: bool,
    /// When true, storage element type is `atomic<…>` (int/uint buffers used with atomics).
    pub atomic: bool,
}

/// One vertex-buffer attribute slot.
#[derive(Debug, Clone)]
pub struct GpuVertexAttr {
    pub location: u32,
    pub format: &'static str,
    pub offset: u32,
}

/// Metadata for one `@compute` kernel.
#[derive(Debug, Clone)]
pub struct GpuKernelInfo {
    pub name: String,
    pub entry: String,
    pub workgroup: (u32, u32, u32),
    pub bindings: Vec<GpuBinding>,
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
    /// Vertex stage only: attribute layout for the first vertex-struct parameter.
    pub vertex_layout: Vec<GpuVertexAttr>,
    pub vertex_stride: u32,
    /// Dream type name of the VS return / FS first param interface struct (empty if none).
    pub interface_ty: String,
    /// Fragment stage: number of `@location` color targets (1 for bare `GpuVec4` return).
    pub color_targets: u32,
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
