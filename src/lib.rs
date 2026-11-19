/// The compiler release: the tag the release workflow builds from, else the crate version.
pub const VERSION: &str = match option_env!("DREAM_RELEASE_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

pub mod driver;
#[cfg(feature = "native")]
pub mod execution;

// Front-end leaves re-exported for the CLI/LSP facade.
pub use dream_diagnostics as diagnostics;
pub use dream_syntax as syntax;
pub use dream_text as text;
