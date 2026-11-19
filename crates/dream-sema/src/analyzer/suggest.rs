//! Close-match suggestions for `help:` notes on unresolved names.

/// Up to three `candidates` that look like a misspelling of `wanted`, closest first. Matching
/// ignores case; a transposition counts as one edit, and longer names tolerate more edits.
/// Names starting with `_` are private helpers and never suggested.
pub(super) fn similar_names<'c>(
    wanted: &str,
    candidates: impl IntoIterator<Item = &'c str>,
) -> Vec<String> {
    let want = wanted.to_lowercase();
    let limit = match want.chars().count() {
        0..=2 => 0,
        3..=5 => 1,
        _ => 2,
    };
    let mut found: Vec<(usize, &str)> = candidates
        .into_iter()
        .filter(|c| *c != wanted && !c.starts_with('_'))
        .filter_map(|c| {
            let lower = c.to_lowercase();
            let distance = strsim::osa_distance(&want, &lower);
            let extends = want.chars().count() >= 3 && lower.starts_with(&want);
            (distance <= limit || extends).then_some((distance, c))
        })
        .collect();
    found.sort_unstable();
    found.dedup_by(|a, b| a.1 == b.1);
    found
        .into_iter()
        .take(3)
        .map(|(_, c)| c.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::similar_names;

    #[test]
    fn suggests_typos_and_extensions_but_not_short_noise() {
        assert_eq!(similar_names("lenght", ["length", "len"]), ["length"]);
        assert_eq!(similar_names("valeu", ["value", "values"]), ["value"]);
        assert_eq!(
            similar_names("sor", ["sort", "sort_by", "zip"]),
            ["sort", "sort_by"]
        );
        assert!(similar_names("x", ["y", "xs"]).is_empty());
        assert!(similar_names("prnt", ["_prnt2"]).is_empty());
    }
}
