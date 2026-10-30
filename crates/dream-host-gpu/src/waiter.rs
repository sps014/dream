//! Per-device completion threads. A job waits for its submission's fence without the host lock,
//! then finishes under the lock (readback copies, deferred releases) and resolves the guest
//! future through the core host's foreign-completion path.

use crate::error::{self, HostResult};
use crate::ffi;
use crate::state::{self, DeviceCore, State};
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

/// Runs under the host lock once the fence is reached (or waiting failed); returns the value
/// the guest future resolves to.
pub type Finish = Box<dyn FnOnce(&mut State, HostResult<()>) -> u64 + Send>;

pub struct Job {
    /// Keeps the device alive while the job is queued, even after the guest released it.
    pub core: Arc<DeviceCore>,
    pub fence: u64,
    pub future: usize,
    pub finish: Finish,
}

pub struct Waiter {
    jobs: Sender<Job>,
}

impl Waiter {
    /// Starts a device's completion thread. It exits once the device is dropped (every guest
    /// handle of it and of its resources released) and every queued job finished.
    pub fn spawn() -> Waiter {
        let (jobs, queue) = channel::<Job>();
        std::thread::Builder::new()
            .name("dream-gpu-completion".into())
            .spawn(move || {
                for job in queue {
                    let core = job.core;
                    let waited = wait_for(&core, job.fence);
                    let value = {
                        let mut state = state::lock();
                        if waited.is_ok() {
                            core.mark_completed(job.fence);
                            state.collect(&core);
                        }
                        (job.finish)(&mut state, waited)
                    };
                    complete(job.future, value);
                }
            })
            .expect("spawning the GPU completion thread");
        Waiter { jobs }
    }

    /// Callers hold the host lock, so a job the thread can no longer take finishes elsewhere.
    pub fn push(&self, job: Job) {
        if let Err(rejected) = self.jobs.send(job) {
            let job = rejected.0;
            std::thread::spawn(move || {
                let value = (job.finish)(
                    &mut state::lock(),
                    Err(error::HostError::new(
                        error::Code::DeviceLost,
                        "the device's completion thread stopped",
                    )),
                );
                complete(job.future, value);
            });
        }
    }
}

/// Waits in slices so a device lost mid-wait (whose fence never advances) ends the wait.
fn wait_for(core: &DeviceCore, fence: u64) -> HostResult<()> {
    const SLICE_NS: u64 = 100_000_000;
    loop {
        core.alive()?;
        match unsafe { ffi::dgpu_fence_wait(core.raw(), fence, SLICE_NS) } {
            ffi::TIMEOUT => continue,
            status => {
                error::check(status)?;
                return core.alive();
            }
        }
    }
}

type CompletionHook = fn(usize, u64);

static HOOK: std::sync::OnceLock<CompletionHook> = std::sync::OnceLock::new();

/// Resolves a guest future (or, in host tests without a guest, reports to the test hook).
pub fn complete(future: usize, value: u64) {
    // Internal submissions collect resources without publishing a guest future.
    if future == 0 { return; }
    match HOOK.get() {
        Some(hook) => hook(future, value),
        None => dream_host_abi::complete_foreign_future(
            dream_host_abi::ForeignFuture::new(future as dream_host_abi::DreamPtr),
            value,
        ),
    }
}

pub fn set_completion_hook(hook: CompletionHook) {
    let _ = HOOK.set(hook);
}

pub fn has_completion_hook() -> bool {
    HOOK.get().is_some()
}
