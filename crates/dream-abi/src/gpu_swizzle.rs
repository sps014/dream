//! Parsing for vector swizzle member names (`v.xyz`, `color.rgb`, `uv.yx`).
//!
//! Shared so the analyzer and the WGSL emitter agree on exactly which names are swizzles: if the
//! two ever disagreed, a name one accepted and the other did not would type-check and then fail
//! shader validation, or vice versa.

/// Component order for the two interchangeable spellings. WGSL forbids mixing them in one name.
const POSITIONAL: [char; 4] = ['x', 'y', 'z', 'w'];
const COLOR: [char; 4] = ['r', 'g', 'b', 'a'];

/// Component indices `name` selects, if it is a swizzle valid on a vector of `arity` components.
///
/// Returns `None` for names that are not swizzles at all, for mixed spellings like `xg`, and for
/// components past the end of the source (`.z` on a `vec2`). Single-component names are included,
/// so callers that treat `.x` as an ordinary field read should check the length themselves.
/// Repeats are allowed, matching WGSL: `v.xxxx` broadcasts.
pub fn components(name: &str, arity: usize) -> Option<Vec<usize>> {
    if name.is_empty() || name.len() > 4 || !name.is_ascii() {
        return None;
    }
    let set = if name.starts_with(|c| POSITIONAL.contains(&c)) {
        &POSITIONAL
    } else {
        &COLOR
    };
    name.chars()
        .map(|c| set.iter().position(|&s| s == c).filter(|&i| i < arity))
        .collect()
}

/// Whether `name` is made only of vector component letters, ignoring arity and spelling mixes.
///
/// Used to tell "you swizzled past the end of a vec2" apart from "that field does not exist", so
/// the two get different diagnostics.
pub fn is_component_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 4
        && name
            .chars()
            .all(|c| POSITIONAL.contains(&c) || COLOR.contains(&c))
}

#[cfg(test)]
mod tests {
    use super::{components, is_component_name};

    #[test]
    fn positional_and_color_spellings_agree() {
        assert_eq!(components("xyz", 4), Some(vec![0, 1, 2]));
        assert_eq!(components("rgb", 4), Some(vec![0, 1, 2]));
        assert_eq!(components("wzyx", 4), Some(vec![3, 2, 1, 0]));
        assert_eq!(components("abgr", 4), Some(vec![3, 2, 1, 0]));
    }

    #[test]
    fn repeats_broadcast() {
        assert_eq!(components("xxxx", 2), Some(vec![0, 0, 0, 0]));
        assert_eq!(components("yy", 2), Some(vec![1, 1]));
    }

    #[test]
    fn rejects_components_past_arity() {
        assert_eq!(components("xyz", 2), None);
        assert_eq!(components("w", 3), None);
        assert_eq!(components("xy", 2), Some(vec![0, 1]));
    }

    #[test]
    fn rejects_mixed_spellings() {
        assert_eq!(components("xg", 4), None);
        assert_eq!(components("rx", 4), None);
    }

    #[test]
    fn rejects_non_components() {
        assert_eq!(components("length", 4), None);
        assert_eq!(components("", 4), None);
        assert_eq!(components("xyzwx", 4), None);
        assert!(!is_component_name("pos"));
        assert!(is_component_name("xy"));
        // Arity is not considered here; that is the caller's distinction to draw.
        assert!(is_component_name("xyzw"));
    }
}
