use super::model::SourceMapping;

/// The same spelling must be safe on every supported host, including Windows.
pub(super) fn valid_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
        && !std::path::Path::new(path).is_absolute()
}

/// Protects Dream source emission and the compiler-owned manifest namespace.
pub(super) fn valid_artifact_path(path: &str) -> bool {
    valid_relative_path(path)
        && !path.to_ascii_lowercase().ends_with(".dream")
        && path.split('/').next() != Some("outputs.json")
}

/// Chooses the innermost half-open source mapping covering an artifact coordinate.
pub(super) fn mapping_at(
    mappings: &[SourceMapping],
    line: u32,
    column: u32,
) -> Option<&SourceMapping> {
    let point = (line, column);
    mappings
        .iter()
        .filter(|m| (m.line, m.column) <= point && point < (m.end_line, m.end_column))
        .max_by_key(|m| {
            (
                (m.line, m.column),
                std::cmp::Reverse((m.end_line, m.end_column)),
            )
        })
}

/// Rejects reversed ranges and coordinates outside their artifact text.
pub(super) fn valid_mapping(m: &SourceMapping, contents: &str) -> bool {
    let valid = |line: u32, column: u32| {
        line > 0
            && column > 0
            && contents
                .split('\n')
                .nth(line.saturating_sub(1) as usize)
                .is_some_and(|s| column as usize <= s.chars().count() + 1)
    };
    (m.line, m.column) < (m.end_line, m.end_column)
        && valid(m.line, m.column)
        && valid(m.end_line, m.end_column)
}

/// A diagnostic point outside the artifact retains its filename without inventing a span.
pub(super) fn position(
    contents: &str,
    line: u32,
    column: u32,
) -> Option<dream_text::text_span::TextSpan> {
    if line == 0 || column == 0 {
        return None;
    }
    let lines: Vec<_> = contents.split('\n').collect();
    let text = lines.get(line as usize - 1)?;
    if column as usize > text.chars().count() + 1 {
        return None;
    }
    let start = lines[..line as usize - 1]
        .iter()
        .map(|s| s.len() + 1)
        .sum::<usize>()
        + text
            .char_indices()
            .nth(column as usize - 1)
            .map(|(byte, _)| byte)
            .unwrap_or(text.len());
    Some(dream_text::text_span::TextSpan {
        start,
        end: start,
        line_no: line as usize,
        col_no: column as usize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_portable_and_manifest_cannot_be_overwritten() {
        for p in ["x.glsl", "nested/schema.json"] {
            assert!(valid_artifact_path(p));
        }
        for p in [
            "",
            "/a",
            "../a",
            "a/../b",
            "a\\b",
            "C:a",
            "a//b",
            "./a",
            "outputs.json",
            "x.DREAM",
        ] {
            assert!(!valid_artifact_path(p), "{p}");
        }
    }

    #[test]
    fn nested_maps_choose_the_innermost_half_open_range() {
        let maps = vec![
            SourceMapping {
                line: 1,
                column: 1,
                end_line: 3,
                end_column: 1,
                target: "outer".into(),
            },
            SourceMapping {
                line: 2,
                column: 2,
                end_line: 2,
                end_column: 4,
                target: "inner".into(),
            },
        ];
        assert_eq!(
            mapping_at(&maps, 2, 2).map(|m| m.target.as_str()),
            Some("inner")
        );
        assert_eq!(
            mapping_at(&maps, 2, 4).map(|m| m.target.as_str()),
            Some("outer")
        );
        assert!(mapping_at(&maps, 3, 1).is_none());
        assert!(valid_mapping(&maps[1], "a\nabc\n"));
    }
    #[test]
    fn unicode_artifact_columns_produce_valid_byte_offsets() {
        let point = position("a\n😀x", 2, 2).unwrap();
        assert_eq!(point.start, 6);
        assert_eq!(point.col_no, 2);
        assert_eq!(position("a\n😀x", 2, 3).unwrap().start, 7);
        assert!(position("a\n😀x", 2, 4).is_none());
    }
}
