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
    pub format: wgpu::TextureFormat,
    pub cpu: Vec<u8>,
    pub gpu: Option<wgpu::Texture>,
    /// Cached default view; cleared when `gpu` is recreated.
    pub view: Option<wgpu::TextureView>,
    pub storage: bool,
    pub depth: bool,
    pub layers: u32,
    /// GPU mip chain length. `1` until `texture_generate_mipmaps`; recreate paths must honor this
    /// so a later `ensure_texture` / blit does not wipe the chain back to a single level.
    pub mip_levels: u32,
    pub dirty_cpu: bool,
}

pub struct SampEntry {
    pub filter: i32,
    pub address: i32,
    pub mip_filter: i32,
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
    pub uniform_slot: u32,
}

pub struct ComputePipe {
    pub pipeline: wgpu::ComputePipeline,
    pub bgl: wgpu::BindGroupLayout,
    /// Pool of 256-byte uniform buffers so batched dispatches don't clobber each other.
    pub uniform_pool: Vec<wgpu::Buffer>,
    /// Next pool slot to use; reset at the start of each submit/dispatch.
    pub uniform_cursor: usize,
    pub bg_cache: IndexMap<ComputeBgKey, wgpu::BindGroup>,
}

pub struct RenderPipe {
    pub pipeline: wgpu::RenderPipeline,
    pub bgl: Option<wgpu::BindGroupLayout>,
    /// Uniform binding slots declared by VS/FS (Dream packs draw uniforms into each).
    pub uniform_bindings: Vec<u32>,
    /// Reusable 256-byte uniform buffer for draw uniforms.
    pub uniform_buf: Option<wgpu::Buffer>,
    pub depth_enabled: bool,
    pub sample_count: u32,
    pub format: wgpu::TextureFormat,
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
    pub depth: Option<wgpu::Texture>,
    pub window: Option<Arc<winit::window::Window>>,
    pub surface: Option<wgpu::Surface<'static>>,
    pub config: Option<wgpu::SurfaceConfiguration>,
    /// Acquired swapchain frame drawn into by the last render pass; presented by `present`.
    pub pending_frame: Option<wgpu::SurfaceTexture>,
    pub input: super::input::InputState,
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
    pub surfaces: IndexMap<i32, SurfaceEntry>,
    pub render_format: wgpu::TextureFormat,
    pub blit: Option<BlitPipe>,
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
            surfaces: IndexMap::new(),
            render_format: wgpu::TextureFormat::Bgra8Unorm,
            blit: None,
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
