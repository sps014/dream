pub mod driver;
#[cfg(feature = "native")]
pub mod execution;
#[cfg(feature = "native")]
#[allow(unused_imports)]
pub use execution::native_c::abi as __native_c_host_abi;
#[cfg(feature = "native")]
#[allow(unused_imports)]
pub use execution::native_c::webview as __native_c_webview;

// Front-end leaves re-exported for the CLI/LSP facade.
pub use dream_diagnostics as diagnostics;
pub use dream_syntax as syntax;
pub use dream_text as text;
