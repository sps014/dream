//! Host state behind one process-wide lock: the handle table, and the released resources still
//! waiting for the GPU. Every glue call that touches a device runs under this lock; only fence
//! waits run outside it.

use crate::backend::Backend;
use crate::bundle;
use crate::error::{Code, HostError, HostResult};
use crate::ffi;
use dream_host_abi::handles::HandleTable;
use crate::waiter::{Finish, Job, Waiter};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

/// One slang-rhi device. Resources hold it alive, so it is destroyed only after the last of them
/// (and its completion thread) let go.
pub struct DeviceCore {
    raw: *mut ffi::DgpuDevice,
    pub backend: Backend,
    pub features: u32,
    completed: AtomicU64,
    simulated_loss: Mutex<Option<String>>,
    waiter: Waiter,
}

// The glue serializes device use through the host lock; fence waits are thread-safe.
unsafe impl Send for DeviceCore {}
unsafe impl Sync for DeviceCore {}

impl DeviceCore {
    pub fn new(raw: *mut ffi::DgpuDevice, backend: Backend) -> Self {
        let features = unsafe { ffi::dgpu_device_features(raw) };
        Self {
            raw,
            backend,
            features,
            completed: AtomicU64::new(0),
            simulated_loss: Mutex::new(None),
            waiter: Waiter::spawn(),
        }
    }

    /// Queues a job on the device's completion thread. Resources keep their device alive, so
    /// this works after the guest released the device handle itself.
    pub fn push(self: &Arc<Self>, fence: u64, future: usize, finish: Finish) {
        self.waiter.push(Job {
            core: self.clone(),
            fence,
            future,
            finish,
        });
    }

    pub fn raw(&self) -> *mut ffi::DgpuDevice {
        self.raw
    }

    pub fn completed(&self) -> u64 {
        self.completed.load(Ordering::Acquire)
    }

    pub fn mark_completed(&self, value: u64) {
        self.completed.fetch_max(value, Ordering::AcqRel);
    }

    pub fn simulate_loss(&self, reason: &str) {
        if let Ok(mut loss) = self.simulated_loss.lock() {
            *loss = Some(reason.to_string());
        }
    }

    /// `Err(EDEVICELOST)` once the driver (or a test) took the device away.
    pub fn alive(&self) -> HostResult<()> {
        if let Some(reason) = self.simulated_loss.lock().ok().and_then(|l| l.clone()) {
            return Err(HostError::new(Code::DeviceLost, reason));
        }
        if unsafe { ffi::dgpu_device_lost(self.raw) } != 0 {
            return Err(HostError::new(Code::DeviceLost, ffi::message()));
        }
        Ok(())
    }

    pub fn has_feature(&self, bit: u32) -> bool {
        self.features & (1 << bit) != 0
    }
}

impl Drop for DeviceCore {
    fn drop(&mut self) {
        unsafe { ffi::dgpu_device_destroy(self.raw) }
    }
}

/// A slang-rhi buffer, texture or sampler reference.
pub struct Obj(*mut c_void);

unsafe impl Send for Obj {}

impl Obj {
    /// Takes ownership of one reference returned by the glue.
    pub fn from_raw(raw: *mut c_void) -> Self {
        Self(raw)
    }

    pub fn raw(&self) -> *mut c_void {
        self.0
    }

    /// A second owner of the same object.
    pub fn share(&self) -> Self {
        unsafe { ffi::dgpu_object_retain(self.0) };
        Self(self.0)
    }
}

impl Drop for Obj {
    fn drop(&mut self) {
        unsafe { ffi::dgpu_object_release(self.0) }
    }
}

macro_rules! owned_pointer {
    ($name:ident, $ty:ty, $release:path) => {
        pub struct $name(*mut $ty);

        unsafe impl Send for $name {}

        impl $name {
            pub fn from_raw(raw: *mut $ty) -> Self {
                Self(raw)
            }

            pub fn raw(&self) -> *mut $ty {
                self.0
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                unsafe { $release(self.0) }
            }
        }
    };
}

owned_pointer!(Program, ffi::DgpuProgram, ffi::dgpu_program_release);
owned_pointer!(Pipeline, ffi::DgpuPipeline, ffi::dgpu_pipeline_release);
owned_pointer!(Surface, ffi::DgpuSurface, ffi::dgpu_surface_release);

pub struct BufferEntry {
    pub obj: Obj,
    pub size: u64,
}

pub struct TextureEntry {
    pub obj: Obj,
    pub layers: u32,
    pub mips: u32,
    pub storage: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineKind {
    Compute,
    Render,
    RayTracing,
}

pub struct PipelineEntry {
    pub pipeline: Pipeline,
    /// Kept alive for as long as the pipeline built from it.
    pub _program: Program,
    pub kind: PipelineKind,
    /// Parameters per program entry (compute: one stage; render: vertex then fragment; ray
    /// tracing: ray generation first, then shaders that take no parameters).
    pub stages: Vec<Vec<bundle::Param>>,
}

pub struct AccelerationStructureEntry {
    pub obj: Obj,
    pub top_level: bool,
    /// The bottom levels a top-level structure's instances point at, kept alive with it.
    pub bottom_levels: Vec<Obj>,
}

pub struct SurfaceEntry {
    pub surface: Surface,
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub vsync: bool,
    /// The acquired image's texture handle until present.
    pub image: Option<i32>,
}

pub enum Kind {
    Device,
    Library(Arc<bundle::Target>),
    Buffer(BufferEntry),
    Texture(TextureEntry),
    Sampler(Obj),
    Pipeline(PipelineEntry),
    Surface(SurfaceEntry),
    AccelerationStructure(AccelerationStructureEntry),
}

impl Kind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Device => "device",
            Self::Library(_) => "shader library",
            Self::Buffer(_) => "buffer",
            Self::Texture(_) => "texture",
            Self::Sampler(_) => "sampler",
            Self::Pipeline(_) => "pipeline",
            Self::Surface(_) => "surface",
            Self::AccelerationStructure(_) => "acceleration structure",
        }
    }
}

pub struct Entry {
    // Declared first so the GPU object is released before the device reference.
    pub kind: Kind,
    /// The owning device's handle (a device's own handle for devices).
    pub device: i32,
    /// Fence value of the last submission that used the resource.
    pub last_use: u64,
    pub core: Arc<DeviceCore>,
}

#[derive(Default)]
pub struct State {
    pub handles: HandleTable<Entry>,
    /// Released by the guest, still referenced by unfinished GPU work.
    deferred: Vec<Entry>,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State::default()));

pub fn lock() -> MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl State {
    pub fn insert(&mut self, entry: Entry) -> HostResult<i32> {
        self.handles
            .insert(entry)
            .ok_or_else(|| HostError::new(Code::OutOfMemory, "GPU handles are exhausted"))
    }

    pub fn entry(&self, handle: i32) -> HostResult<&Entry> {
        self.handles
            .get(handle)
            .ok_or_else(|| HostError::invalid("the GPU resource was released"))
    }

    pub fn entry_mut(&mut self, handle: i32) -> HostResult<&mut Entry> {
        self.handles
            .get_mut(handle)
            .ok_or_else(|| HostError::invalid("the GPU resource was released"))
    }

    /// The live device behind `handle`.
    pub fn device(&self, handle: i32) -> HostResult<Arc<DeviceCore>> {
        let entry = self.entry(handle)?;
        if !matches!(entry.kind, Kind::Device) {
            return Err(HostError::invalid(format!(
                "handle {handle} is a {}, not a device",
                entry.kind.name()
            )));
        }
        entry.core.alive()?;
        Ok(entry.core.clone())
    }

    /// The entry behind `handle`, checked to belong to `device`.
    pub fn owned(&self, handle: i32, device: i32, what: &str) -> HostResult<&Entry> {
        let entry = self
            .handles
            .get(handle)
            .ok_or_else(|| HostError::invalid(format!("the {what} was released")))?;
        if entry.device != device {
            return Err(HostError::new(Code::WrongDevice, what.to_string()));
        }
        Ok(entry)
    }

    pub fn touch(&mut self, handle: i32, fence: u64) {
        if let Some(entry) = self.handles.get_mut(handle) {
            entry.last_use = entry.last_use.max(fence);
        }
    }

    /// Drops the guest's reference. Resources stay alive until their last submission finished.
    pub fn release(&mut self, handle: i32) {
        let Some(entry) = self.handles.remove(handle) else {
            return;
        };
        if let Kind::Surface(surface) = &entry.kind
            && let Some(image) = surface.image
        {
            self.release(image);
        }
        if entry.last_use > entry.core.completed() {
            self.deferred.push(entry);
        }
    }

    /// Frees deferred resources of `core` whose work has finished.
    pub fn collect(&mut self, core: &Arc<DeviceCore>) {
        let completed = core.completed();
        self.deferred
            .retain(|e| !(Arc::ptr_eq(&e.core, core) && e.last_use <= completed));
    }

    #[doc(hidden)]
    pub fn deferred_len(&self) -> usize {
        self.deferred.len()
    }
}
