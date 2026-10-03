//! Which source files are library code rather than the program being built: the embedded stdlib
//! (virtual `<std>/…` paths) and installed dependency packages (`dream_packages/<pkg>/…`).

use std::path::Path;

/// Prefix of every embedded stdlib file's virtual path.
pub const STD_PATH_PREFIX: &str = "<std>/";

/// Directory, next to a project's `dream.toml`, that dependency packages are installed into.
pub const PACKAGES_DIR: &str = "dream_packages";

pub fn is_std_source(path: &str) -> bool {
    path.starts_with(STD_PATH_PREFIX)
}

/// True for stdlib and dependency-package sources. Panics inside library functions report the
/// calling program's line instead of the library's.
pub fn is_library_source(path: &str) -> bool {
    is_std_source(path)
        || Path::new(path)
            .components()
            .any(|c| c.as_os_str() == PACKAGES_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_std_packages_and_program_sources() {
        assert!(is_library_source("<std>/system/collections/list.dream"));
        assert!(is_library_source(
            "/work/app/dream_packages/json_tools/src/parse.dream"
        ));
        assert!(!is_library_source("/work/app/src/main.dream"));
        assert!(!is_library_source("/work/app/src/dream_packages_notes.dream"));
    }
}
