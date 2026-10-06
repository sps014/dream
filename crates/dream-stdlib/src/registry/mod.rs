mod codegen;
mod collections;
mod core;
mod crypto;
mod encoding;
mod io;
mod json;
mod json_derive;
mod logging;
mod primitives;
mod process;
mod simd;
mod system;
mod task;
mod testing;
mod text;

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
    crypto::PACKAGE,
    process::PACKAGE,
    system::PACKAGE,
    testing::PACKAGE,
    codegen::PACKAGE,
    json_derive::PACKAGE,
    logging::PACKAGE,
];
