mod codegen;
mod collections;
mod core;
mod crypto;
mod desktop;
mod encoding;
mod gpu;
mod io;
mod json;
mod logging;
mod net;
mod primitives;
mod process;
mod simd;
mod system;
mod task;
mod testing;
mod text;
mod webapi;
mod webview;

/// One embedded stdlib package: dotted import name, ordered source files, and package deps.
pub struct StdPackage {
    /// Dotted path users write in `import system.net;`.
    pub name: &'static str,
    /// `(virtual path, source)` pairs in merge order within this package.
    pub files: &'static [(&'static str, &'static str)],
    /// Other packages that must be loaded before this one.
    pub deps: &'static [&'static str],
}

/// Bootstrap packages always merged into every program (no user `import` required).
pub const BOOTSTRAP_PACKAGES: &[&str] = &["system.core", "system.primitives"];

/// All stdlib packages, in global prelude merge order.
pub const STD_PACKAGES: &[StdPackage] = &[
    core::PACKAGE,
    primitives::PACKAGE,
    collections::PACKAGE,
    simd::PACKAGE,
    task::PACKAGE,
    text::PACKAGE,
    encoding::PACKAGE,
    json::PACKAGE,
    gpu::PACKAGE,
    io::PACKAGE,
    net::PACKAGE,
    crypto::PACKAGE,
    process::PACKAGE,
    webview::PACKAGE,
    desktop::PACKAGE,
    system::PACKAGE,
    testing::PACKAGE,
    codegen::PACKAGE,
    logging::PACKAGE,
    webapi::PACKAGE,
];
