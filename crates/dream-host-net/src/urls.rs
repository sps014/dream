use url::{Position, Url};

pub(crate) fn parse(input: &str, base: &str) -> Result<Vec<String>, url::ParseError> {
    let url = if base.is_empty() {
        Url::parse(input)?
    } else {
        Url::parse(base)?.join(input)?
    };
    // Prefixes distinguish absent delimiters from present but empty query/fragment components.
    let mut parts = vec![
        url[..Position::AfterPath].to_string(),
        url.scheme().into(),
        url.host_str().unwrap_or_default().into(),
        url.path().into(),
        url.fragment().map(|v| format!("#{v}")).unwrap_or_default(),
        url.query().map(|v| format!("?{v}")).unwrap_or_default(),
    ];
    for (key, value) in url.query_pairs() {
        parts.push(key.into_owned());
        parts.push(value.into_owned());
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn normalizes_and_resolves_using_the_url_standard() {
        let p = parse("HTTPS://ExAmPle.Com:443/a/../b?q=a+b&q=%2B#", "").unwrap();
        assert_eq!(
            p,
            [
                "https://example.com/b",
                "https",
                "example.com",
                "/b",
                "#",
                "?q=a+b&q=%2B",
                "q",
                "a b",
                "q",
                "+"
            ]
        );
        assert_eq!(
            parse("../users", "https://example.com/v1/nested/").unwrap()[0],
            "https://example.com/v1/users"
        );
        assert!(parse("relative", "").is_err());
        assert!(parse("http://[bad", "").is_err());
    }

    #[test]
    fn preserves_empty_components_and_unicode() {
        assert_eq!(
            parse("data:text/plain,hello ?#", "").unwrap()[0],
            "data:text/plain,hello "
        );
        let p = parse("https://bücher.example/こんにちは?empty=&=value#", "").unwrap();
        assert_eq!(p[2], "xn--bcher-kva.example");
        assert_eq!(&p[6..], ["empty", "", "", "value"]);
        assert_eq!(
            parse("https://example.com/?#", "").unwrap()[4..6],
            ["#", "?"]
        );
    }
}
