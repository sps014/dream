// These C entry points share the guest ABI contract: pointer handles come from
// the validated program's runtime, never arbitrary Rust callers.
#![allow(clippy::missing_safety_doc)]

mod abi;
#[cfg(any(feature = "gpu", feature = "webview"))]
mod app_icon;
#[cfg(feature = "core")]
mod core;
#[cfg(feature = "webview")]
mod desktop;
#[cfg(feature = "gpu")]
mod gpu;
#[cfg(feature = "net")]
mod net;
#[cfg(feature = "webview")]
mod webview;
