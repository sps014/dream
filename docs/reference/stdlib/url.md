# URLs and query parameters

Import `system.net` for URL parsing and relative resolution. Native programs use
Rust's `url` library; Node and browser programs use the host URL API. Parsing
requires an absolute URL and returns `Result<Url, ParseError>`.

```dream
import system;
import system.net;

fun main(): Result<bool, ParseError> {
    let base = Url.parse("HTTPS://Example.COM:443/v1/")?;
    let users = base.resolve("users")?;
    users.query.append("role", "admin");
    users.query.append("role", "editor");
    System.println(users.to_string());
    return Result.Ok(true);
}
```

The example prints `https://example.com/v1/users?role=admin&role=editor`.
Normalization includes scheme/host casing, default ports, dot segments, Unicode
hostnames, and path encoding. Relative resolution follows URL Standard rules:
`/v1/` is a directory base, while `/v1` has a final component replaced by a
relative reference. An absolute reference replaces the entire base.

`scheme` excludes its colon. `host` contains the canonical hostname, including
IPv6 brackets and excluding the port; URLs without a host return an empty string.
`path` remains percent encoded. Serialization preserves present but empty query
and fragment delimiters.

`query` is a mutable ordered collection attached to the URL. `get` returns the
first decoded value, `get_all` copies every value for that key, and `length`
counts pairs, including duplicates. `set` replaces the first occurrence and
removes later duplicates. `remove` removes every matching pair; `clear` removes
all parameters and the query delimiter.

Existing query encoding is preserved until mutation. Mutations serialize decoded
pairs using form encoding: spaces become `+`, literal plus signs become `%2b`,
and empty values retain `=`. URI-component codecs in `system.encoding` have
different plus-sign rules. URL parsing follows the host URL Standard's recovery
rules; it does not impose strict UTF-8 decoding on query values.

Opaque-scheme URLs with unescaped spaces currently serialize differently between
the native parser and some JS hosts. Encode those spaces as `%20` when identical
serialization across targets is required.

For network requests, see [HTTP clients](http.md). Parsing a URL performs no network I/O.
