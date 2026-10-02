//! Native host implementations and guest C ABI, separate from the compiler.

#[cfg(any(feature = "gpu", feature = "webview"))]
mod app_icon;
#[cfg(feature = "core")]
mod crypto;
#[cfg(feature = "webview")]
mod desktop;
mod exports;
#[cfg(feature = "gpu")]
mod gpu;
#[cfg(feature = "net")]
mod http;
#[cfg(feature = "net")]
mod http_server;
#[cfg(feature = "net")]
mod net;
#[cfg(feature = "core")]
mod process_host;
#[cfg(feature = "core")]
mod tz;
#[cfg(feature = "webview")]
mod webview;

#[cfg(test)]
mod contract_tests;
