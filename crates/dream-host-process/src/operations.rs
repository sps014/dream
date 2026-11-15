use dream_host_abi::operations::Operations;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

pub(crate) use dream_host_abi::operations::Failure;
pub(crate) type Output = dream_host_abi::operations::Output<crate::service::OwnedChild>;

pub(crate) static OPERATIONS: Operations<crate::service::OwnedChild> = Operations::new(runtime);

pub(crate) fn runtime() -> Result<&'static Runtime, Failure> {
    static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .on_thread_start(block_broken_pipe)
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| ("EIO", e.clone()))
}

fn block_broken_pipe() {
    #[cfg(unix)]
    {
        // A C executable does not install Rust main's SIGPIPE policy. Block it only on
        // these I/O workers so a closed child stdin returns EPIPE instead of killing the guest.
        let mut signals = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
        // SAFETY: sigemptyset initializes this writable set; subsequent calls use valid constants.
        unsafe {
            if libc::sigemptyset(signals.as_mut_ptr()) == 0 {
                libc::sigaddset(signals.as_mut_ptr(), libc::SIGPIPE);
                libc::pthread_sigmask(libc::SIG_BLOCK, signals.as_ptr(), std::ptr::null_mut());
            }
        }
    }
}
