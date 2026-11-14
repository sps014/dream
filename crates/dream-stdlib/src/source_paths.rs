//! Path conventions for library sources: the embedded stdlib's virtual `<std>/…` paths and the
//! directory dependency packages are installed into.

/// Prefix of every embedded stdlib file's virtual path.
pub const STD_PATH_PREFIX: &str = "<std>/";

/// Directory, next to a project's `dream.toml`, that dependency packages are installed into.
pub const PACKAGES_DIR: &str = "dream_packages";

pub fn is_std_source(path: &str) -> bool {
    path.starts_with(STD_PATH_PREFIX)
}
