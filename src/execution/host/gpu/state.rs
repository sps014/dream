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
    pub storage: bool,
    pub depth: bool,
    pub layers: u32,
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

pub struct ComputePipe {
    pub pipeline: wgpu::ComputePipeline,
    pub bgl: wgpu::BindGroupLayout,
    /// Reusable 256-byte uniform buffer (extents + packed uniforms).
    pub uniform_buf: wgpu::Buffer,
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
    pub width: u32,
    pub height: u32,
    /// Offscreen color target used when there is no window swapchain (or for blit).
    pub color: Option<wgpu::Texture>,
    pub depth: Option<wgpu::Texture>,
    pub window: Option<Arc<winit::window::Window>>,
    pub surface: Option<wgpu::Surface<'static>>,
    pub config: Option<wgpu::SurfaceConfiguration>,
    /// Acquired swapchain frame drawn into by the last render pass; presented by `present`.
    pub pending_frame: Option<wgpu::SurfaceTexture>,
}

pub struct GpuState {
    pub next_id: i32,
    pub ready: bool,
    pub abi: Option<GpuAbi>,
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
    /// Last wgpu uncaptured error; consumed by host calls after submit.
    pub last_error: Option<String>,
}

impl Default for GpuState {
    fn default() -> Self {
        Self {
            next_id: 1,
            ready: false,
            abi: None,
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
            render_format: wgpu::TextureFormat::Bgra8UnormSrgb,
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
        self.map.get(&self.id).expect("gpu state missing for thread")
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
