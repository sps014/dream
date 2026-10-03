use super::*;

/// Converts an AST node's `Rc<str>` source-file tag into the `String` form stored on the
/// diagnostic bag (used to attribute each semantic error to its originating file).
pub(super) fn file_path_string(file_path: &Option<Rc<str>>) -> Option<String> {
    file_path.as_ref().map(|p| p.to_string())
}

/// Compare compile-root and declaring-file paths without requiring both to be canonicalized.
pub(super) fn paths_equal(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let pa = std::path::Path::new(a);
    let pb = std::path::Path::new(b);
    match (pa.canonicalize(), pb.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => pa.file_name() == pb.file_name() && pa.file_name().is_some(),
    }
}
