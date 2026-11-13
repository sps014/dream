use dream_abi::toolchain::PinnedTool;

mod app;
mod audio;
mod cli;
mod codegen;
mod collections;
mod compress;
mod config;
mod core;
mod crypto;
mod csv;
mod drawing;
mod drawing_gpu;
mod encoding;
mod gpu;
mod gpu_shaders;
mod interop;
mod io;
mod js;
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
mod validation;
mod validation_derive;
mod webapi;
mod webapi_auth;
mod webapi_derive;
mod window;

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
    /// The compiler may name declarations in this package. Library packages are ordinary code.
    pub core_language: bool,
    /// Browser and Node host files the package's `@runtime` services need next to the `.wasm`.
    pub web_assets: Option<&'static WebAssets>,
    /// Pinned external tools the package's generators request with `@uses_tool`.
    pub tools: &'static [PinnedTool],
    /// Runtime functions a WebAssembly guest exports while one of the package's host imports is
    /// used, so the JavaScript host can call back into the guest runtime.
    pub guest_exports: &'static [GuestExport],
}

/// A guest runtime function exported for a library package's JavaScript host.
pub struct GuestExport {
    /// Host import field whose use triggers the export.
    pub import: &'static str,
    /// Name the host sees on the module's exports.
    pub export_name: &'static str,
    /// Runtime C function the export forwards to.
    pub symbol: &'static str,
}

/// Prebuilt JavaScript host files a library package ships for browser and Node targets.
pub struct WebAssets {
    /// What the files are, as shown in the error when they are missing.
    pub name: &'static str,
    /// Environment variable that overrides the directory holding the files.
    pub env: &'static str,
    /// Directory beside the installed `runtime/js` sources.
    pub installed_dir: &'static str,
    /// Directory under the Cargo target directory in a development checkout.
    pub development_dir: &'static str,
    /// Files copied next to the emitted module.
    pub files: &'static [&'static str],
    /// How to produce the files when they are missing.
    pub build_hint: &'static str,
}

impl StdPackage {
    /// True when `name` is a core-language package the compiler is allowed to reference.
    pub fn is_core_language(name: &str) -> bool {
        STD_PACKAGES
            .iter()
            .any(|package| package.core_language && package.name == name)
    }
}

/// Bootstrap packages always merged into every program (no user `import` required).
pub const BOOTSTRAP_PACKAGES: &[&str] = &["system.core", "system.primitives"];

/// All stdlib packages, in global prelude merge order.
pub const STD_PACKAGES: &[StdPackage] = &[
    core::PACKAGE,
    js::PACKAGE,
    audio::PACKAGE,
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
    drawing::PACKAGE,
    drawing_gpu::PACKAGE,
    uuid::PACKAGE,
    validation::PACKAGE,
    validation_derive::PACKAGE,
    process::PACKAGE,
    system::PACKAGE,
    app::PACKAGE,
    testing::PACKAGE,
    codegen::PACKAGE,
    json_derive::PACKAGE,
    logging::PACKAGE,
    webapi::PACKAGE,
    webapi_auth::PACKAGE,
    webapi_derive::PACKAGE,
    window::PACKAGE,
    gpu::PACKAGE,
    gpu_shaders::PACKAGE,
];
