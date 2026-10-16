mod cli;
mod codegen;
mod collections;
mod compress;
mod config;
mod core;
mod crypto;
mod csv;
mod encoding;
mod interop;
mod io;
mod json;
mod json_derive;
mod logging;
mod net;
mod primitives;
mod process;
mod simd;
mod system;
mod task;
mod testing;
mod text;
mod uuid;
mod webapi;
mod webapi_derive;

/// One embedded stdlib package: dotted import name, ordered source files, and package deps.
pub struct StdPackage {
    /// Dotted path users write in `import system.io;`.
    pub name: &'static str,
    /// `(virtual path, source)` pairs in merge order within this package.
    pub files: &'static [(&'static str, &'static str)],
    /// Other packages that must be loaded before this one.
    pub deps: &'static [&'static str],
    /// Generator packages whose `@generator`s are registered whenever this package is loaded.
    /// They are never merged into the user's program.
    pub generators: &'static [&'static str],
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
    io::PACKAGE,
    interop::PACKAGE,
    net::PACKAGE,
    cli::PACKAGE,
    config::PACKAGE,
    csv::PACKAGE,
    compress::PACKAGE,
    crypto::PACKAGE,
    uuid::PACKAGE,
    process::PACKAGE,
    system::PACKAGE,
    testing::PACKAGE,
    codegen::PACKAGE,
    json_derive::PACKAGE,
    logging::PACKAGE,
    webapi::PACKAGE,
    webapi_derive::PACKAGE,
];
