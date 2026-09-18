//! GPU host state (CPU mirrors + wgpu resources + ABI).
//! Keyed by OS thread so parallel e2e cases (rayon) isolate ABI/ids. State lives in a
//! process-global map (not `thread_local!`) so wgpu Drop does not touch dying TLS.

use super::abi::GpuAbi;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread::ThreadId;
use wgpu;

pub struct BufEntry {
    pub cpu: Vec<u8>,
    pub gpu: Option<wgpu::Buffer>,
    pub usage: wgpu::BufferUsages,
    /// Usage flags the live `gpu` buffer was created with (if any).
    pub created_usage: wgpu::BufferUsages,
    pub dirty_cpu: bool,
}

pub struct TexEntry {
    pub width: u32,
    pub height: u32,
    /// Resolved once at creation: the shared format spec plus its `wgpu` code.
    pub format: super::formats::ResolvedFormat,
    pub cpu: Vec<u8>,
    pub gpu: Option<wgpu::Texture>,
    /// Cached default view; cleared when `gpu` is recreated.
    pub view: Option<wgpu::TextureView>,
    /// Needs `STORAGE_BINDING` usage: either created with a `storage_access`, or later bound to a
    /// `texture_storage_*` slot.
    pub storage: bool,
    pub dimension: wgpu::TextureDimension,
    /// Array layers for 1D/2D, or depth slices for 3D.
    pub layers: u32,
    /// How bindings view the layers. A 6-layer 2D texture is a cubemap only if it says so here —
    /// `textureSample` on a `texture_cube<f32>` binding fails validation against a `D2Array` view.
    pub view_dimension: wgpu::TextureViewDimension,
    /// GPU mip chain length. `1` until `texture_generate_mipmaps`; recreate paths must honor this
    /// so a later `ensure_texture` / blit does not wipe the chain back to a single level.
    pub mip_levels: u32,
    pub sample_count: u32,
    pub dirty_cpu: bool,
}

impl TexEntry {
    pub fn depth(&self) -> bool {
        self.format.is_depth()
    }

    pub fn wgpu_format(&self) -> wgpu::TextureFormat {
        self.format.wgpu
    }
}

pub struct SampEntry {
    pub mag_filter: i32,
    pub min_filter: i32,
    pub mip_filter: i32,
    pub address: [i32; 3],
    pub lod: (f32, f32),
    /// `GpuCompareFunction` code for a comparison (shadow) sampler, `None` otherwise. A comparison
    /// sampler binds only to `sampler_comparison`, so the layout depends on this.
    pub compare: Option<i32>,
    pub max_anisotropy: u16,
    pub gpu: Option<wgpu::Sampler>,
}

pub struct RawShader {
    pub source: String,
    pub entry: String,
}

pub enum PassOp {
    Dispatch {
        kernel: String,
        buffer_ids: Vec<i32>,
        texture_ids: Vec<i32>,
        sampler_ids: Vec<i32>,
        ex: i32,
        ey: i32,
        ez: i32,
        uniforms: Vec<u8>,
    },
    DispatchIndirect {
        kernel: String,
        buffer_ids: Vec<i32>,
        texture_ids: Vec<i32>,
        sampler_ids: Vec<i32>,
        indirect_id: i32,
        offset: i32,
    },
}

/// Cache key for compute bind groups (resource ids + which uniform pool slot).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ComputeBgKey {
    pub kernel: String,
    pub buffer_ids: Vec<i32>,
    pub texture_ids: Vec<i32>,
    pub sampler_ids: Vec<i32>,
}

pub struct ComputePipe {
    pub pipeline: wgpu::ComputePipeline,
    /// Dense per-`@group` layouts; index `i` is `@group(i)`.
    pub bgls: Vec<wgpu::BindGroupLayout>,
    /// Byte size of the kernel's uniform block, `0` when it declares none.
    pub uniform_size: u32,
    /// One bind group per declared `@group`, in ascending group order.
    pub bg_cache: IndexMap<ComputeBgKey, Vec<wgpu::BindGroup>>,
}

/// Everything needed to re-create a render pipeline for a different color-target format.
/// A pipeline's color format is baked in at creation, but the same Dream pipeline handle is used
/// for both the swapchain (`bgra8unorm`) and offscreen textures (`rgba8unorm`, `rgba16float`).
pub struct RenderPipeBuild {
    pub vs_mod: wgpu::ShaderModule,
    pub fs_mod: wgpu::ShaderModule,
    pub vs_entry: String,
    pub fs_entry: String,
    pub layout: wgpu::PipelineLayout,
    /// One entry per vertex buffer slot. `wgpu::VertexBufferLayout` borrows its attributes, so
    /// they are owned here and the layouts are rebuilt for each pipeline variant.
    pub vertex_buffers: Vec<VertexBufferBuild>,
    pub color_targets: u32,
    pub topology: wgpu::PrimitiveTopology,
    pub front_face: wgpu::FrontFace,
    pub cull_mode: Option<wgpu::Face>,
    pub blend: Option<wgpu::BlendState>,
    pub depth_stencil: Option<wgpu::DepthStencilState>,
    pub sample_count: u32,
}

pub struct VertexBufferBuild {
    pub stride: u32,
    pub step_mode: wgpu::VertexStepMode,
    pub attributes: Vec<wgpu::VertexAttribute>,
}

pub struct RenderPipe {
    /// Pipeline variants keyed by color-target format, minted on first use.
    pub variants: IndexMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    pub build: RenderPipeBuild,
    /// Dense per-`@group` layouts; index `i` is `@group(i)`.
    pub bgls: Vec<wgpu::BindGroupLayout>,
    /// Bindings declared by VS/FS, grouped by `@group` and deduped by `(group, binding)`.
    pub groups: Vec<super::binds::BindGroupPlan>,
    /// Byte size of the uniform block VS/FS share, `0` when neither declares one.
    pub uniform_size: u32,
    pub depth_enabled: bool,
    pub sample_count: u32,
}

/// A `GpuBindGroup` handle: a validated resource set for one `@group` of one pipeline.
///
/// Resource *ids* are stored rather than a built `wgpu::BindGroup` because the uniform block is
/// supplied per draw, so the final bind group depends on which uniform pool slot the draw got.
pub struct BindGroupEntry {
    pub pipeline_id: i32,
    pub group: u32,
    pub buffer_ids: Vec<i32>,
    pub texture_ids: Vec<i32>,
    pub sampler_ids: Vec<i32>,
}

/// Cache key for a realized render bind group. The uniform block is reached through a dynamic
/// offset rather than baked into the group, so an entry stays valid for every draw that uses the
/// same resources — which is the whole point of building a `GpuBindGroup` once.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RenderBgKey {
    pub pipeline_id: i32,
    pub group: u32,
    pub bind_group_id: i32,
}

pub struct SurfaceEntry {
    /// Swapchain / wgpu size in physical pixels.
    pub width: u32,
    pub height: u32,
    /// Pointer + `width()`/`height()` space (logical / create·configure size). Matches web canvas CSS pixels.
    pub client_width: u32,
    pub client_height: u32,
    /// Offscreen color target used when there is no window swapchain (or for blit).
    pub color: Option<wgpu::Texture>,
    /// Multisampled color target resolved into the swapchain when a pipeline asks for MSAA.
    pub msaa: Option<wgpu::Texture>,
    pub msaa_samples: u32,
    pub depth: Option<wgpu::Texture>,
    /// Sample count `depth` was created with; a pass at a different count recreates it.
    pub depth_samples: u32,
    pub window: Option<Arc<winit::window::Window>>,
    pub surface: Option<wgpu::Surface<'static>>,
    pub config: Option<wgpu::SurfaceConfiguration>,
    /// Acquired swapchain frame drawn into by the last render pass; presented by `present`.
    pub pending_frame: Option<wgpu::SurfaceTexture>,
    pub input: super::input::InputState,
    pub present_mode: wgpu::PresentMode,
    pub alpha_mode: wgpu::CompositeAlphaMode,
    pub color_space: i32,
}

pub struct BlitPipe {
    pub pipeline: wgpu::RenderPipeline,
    pub bgl: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
    pub format: wgpu::TextureFormat,
    /// Cached blit bind groups keyed by source texture id.
    pub bg_by_tex: IndexMap<i32, wgpu::BindGroup>,
}

pub struct GpuState {
    pub next_id: i32,
    pub ready: bool,
    pub abi: Option<GpuAbi>,
    /// Set when sibling `.abi.json` is missing or has no `gpu` section; logged once on first use.
    pub missing_gpu_abi: Option<String>,
    pub warned_missing_gpu_abi: bool,
    pub instance: Option<wgpu::Instance>,
    pub adapter: Option<wgpu::Adapter>,
    pub device: Option<wgpu::Device>,
    pub queue: Option<wgpu::Queue>,
    pub buffers: IndexMap<i32, BufEntry>,
    pub textures: IndexMap<i32, TexEntry>,
    pub samplers: IndexMap<i32, SampEntry>,
    pub shaders: IndexMap<i32, RawShader>,
    pub passes: IndexMap<i32, Vec<PassOp>>,
    pub compute_pipes: IndexMap<String, ComputePipe>,
    pub render_pipes: IndexMap<i32, RenderPipe>,
    pub bind_groups: IndexMap<i32, BindGroupEntry>,
    pub render_bg_cache: IndexMap<RenderBgKey, wgpu::BindGroup>,
    pub surfaces: IndexMap<i32, SurfaceEntry>,
    pub render_format: wgpu::TextureFormat,
    pub blit: Option<BlitPipe>,
    /// Per-draw uniform blocks for the frame being recorded, bound at dynamic offsets.
    pub uniform_ring: super::uniform_ring::UniformRing,
    /// Last wgpu uncaptured error; consumed by host calls after submit.
    pub last_error: Option<String>,
}

impl Default for GpuState {
    fn default() -> Self {
        Self {
            next_id: 1,
            ready: false,
            abi: None,
            missing_gpu_abi: None,
            warned_missing_gpu_abi: false,
            instance: None,
            adapter: None,
            device: None,
            queue: None,
            buffers: IndexMap::new(),
            textures: IndexMap::new(),
            samplers: IndexMap::new(),
            shaders: IndexMap::new(),
            passes: IndexMap::new(),
            compute_pipes: IndexMap::new(),
            render_pipes: IndexMap::new(),
            bind_groups: IndexMap::new(),
            render_bg_cache: IndexMap::new(),
            surfaces: IndexMap::new(),
            render_format: wgpu::TextureFormat::Bgra8Unorm,
            blit: None,
            uniform_ring: super::uniform_ring::UniformRing::default(),
            last_error: None,
        }
    }
}

impl GpuState {
    pub fn alloc_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn invalidate_blit_tex(&mut self, tex_id: i32) {
        if let Some(blit) = self.blit.as_mut() {
            blit.bg_by_tex.shift_remove(&tex_id);
        }
    }

    /// Log once when kernels/shaders need ABI metadata that was not loaded.
    pub fn warn_if_gpu_abi_missing(&mut self) {
        if self.abi.is_some() || self.warned_missing_gpu_abi {
            return;
        }
        if let Some(reason) = self.missing_gpu_abi.as_ref() {
            eprintln!(
                "Dream GPU: {}; kernels/shaders will fail validation",
                reason
            );
            self.warned_missing_gpu_abi = true;
        }
    }

    /// Drop GPU resources while the thread is still alive (safe for wgpu).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn set_last_error(&mut self, msg: String) {
        self.last_error = Some(msg);
    }

    /// Drops the wgpu device and every GPU-side cache, keeping CPU mirrors, windows, and ABI
    /// metadata so `try_init` can recover without forgetting what the program already allocated.
    pub fn drop_gpu_device(&mut self) {
        self.ready = false;
        self.device = None;
        self.queue = None;
        self.blit = None;
        self.compute_pipes.clear();
        self.render_pipes.clear();
        self.bind_groups.clear();
        self.render_bg_cache.clear();
        self.uniform_ring = super::uniform_ring::UniformRing::default();
        for buf in self.buffers.values_mut() {
            buf.gpu = None;
            buf.created_usage = wgpu::BufferUsages::empty();
        }
        for tex in self.textures.values_mut() {
            tex.gpu = None;
            tex.view = None;
        }
        for samp in self.samplers.values_mut() {
            samp.gpu = None;
        }
        for surf in self.surfaces.values_mut() {
            surf.color = None;
            surf.msaa = None;
            surf.depth = None;
            surf.pending_frame = None;
            surf.config = None;
        }
    }
}

fn states() -> &'static Mutex<HashMap<ThreadId, GpuState>> {
    static CELL: OnceLock<Mutex<HashMap<ThreadId, GpuState>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct StateGuard {
    map: MutexGuard<'static, HashMap<ThreadId, GpuState>>,
    id: ThreadId,
}

impl Deref for StateGuard {
    type Target = GpuState;
    fn deref(&self) -> &GpuState {
        self.map
            .get(&self.id)
            .expect("gpu state missing for thread")
    }
}

impl DerefMut for StateGuard {
    fn deref_mut(&mut self) -> &mut GpuState {
        self.map
            .get_mut(&self.id)
            .expect("gpu state missing for thread")
    }
}

/// Lock this thread's GPU state slot (exclusive over the whole registry while held).
pub fn lock_state() -> StateGuard {
    let id = std::thread::current().id();
    let mut map = states().lock().unwrap_or_else(|e| e.into_inner());
    map.entry(id).or_default();
    StateGuard { map, id }
}

pub const ERR_UNAVAILABLE: i32 = 1;
pub const ERR_TIMEOUT: i32 = 2;
pub const ERR_VALIDATION: i32 = 3;
pub const ERR_OTHER: i32 = 4;
/// The request is well-formed but the adapter lacks the feature or limit it needs — a
/// block-compressed format on a device without that family, say. Distinct from `ERR_VALIDATION`
/// because the fix is to pick a different resource, not to correct the call.
pub const ERR_UNSUPPORTED: i32 = 5;
/// The GPU device was lost (driver reset, tab discarded, thermal kill). `try_init` again to
/// recover; previously created GPU resources are gone.
pub const ERR_DEVICE_LOST: i32 = 6;
