use dream_host_abi::operations::{Detach, Operations};
use std::sync::OnceLock;
use tokio::runtime::Runtime;

pub(crate) use dream_host_abi::operations::Failure;
pub(crate) type Output = dream_host_abi::operations::Output<OwnedResource>;

pub(crate) static OPERATIONS: Operations<OwnedResource> = Operations::new(runtime);

/// A guest-visible resource closed on drop unless the guest claimed it.
pub(crate) struct OwnedResource {
    id: i32,
    close: fn(i32),
    detached: bool,
}
impl OwnedResource {
    pub fn new(id: i32, close: fn(i32)) -> Self {
        Self {
            id,
            close,
            detached: false,
        }
    }
}
impl Detach for OwnedResource {
    fn detach(&mut self) {
        self.detached = true;
    }
}
impl Drop for OwnedResource {
    fn drop(&mut self) {
        if !self.detached {
            (self.close)(self.id);
        }
    }
}

/// The one Tokio runtime for every network capability (sockets, HTTP, servers, WebSockets).
pub(crate) fn runtime() -> Result<&'static Runtime, Failure> {
    static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            // Transitive TLS clients enable both providers; rustls cannot infer a default.
            let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| ("EIO", e.clone()))
}
