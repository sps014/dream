---
title: URLs
summary: Parse URLs and work with query parameters.
icon: material/link-variant
api: reference/api/system-net.md
---

# URLs and query parameters

Import `system.net` for URL parsing and relative resolution. Native programs use
Rust's `url` library; Node and browser programs use the host URL API. Parsing
requires an absolute URL and returns `Result<Url, ParseError>`.

```dream
import system;
import system.net;

fun main(): Result<Unit, ParseError> {
    let base = Url.parse("HTTPS://Example.COM:443/v1/")?;
    let users = base.resolve("users")?;
    users.query.append("role", "admin");
    users.query.append("role", "editor");
    System.println(users.to_string());
    return Result.Ok(Unit());
}
```

The example prints `https://example.com/v1/users?role=admin&role=editor`.
Normalization includes scheme/host casing, default ports, dot segments, Unicode
hostnames, and path encoding. Relative resolution follows URL Standard rules:
`/v1/` is a directory base, while `/v1` has a final component replaced by a
relative reference. An absolute reference replaces the entire base.

## Url members

| Member | Returns | Effect |
|---|---|---|
| `Url.parse(input)` | `Result<Url, ParseError>` | Parses and normalizes an absolute URL. |
| `resolve(reference)` | `Result<Url, ParseError>` | Resolves a reference against this URL. |
| `scheme` | `string` | Lowercase scheme without the colon. |
| `host` | `string` | Canonical hostname without the port. |
| `path` | `string` | Percent-encoded path. |
| `query` | `UrlQuery` | Mutable query parameters attached to this URL. |
| `to_string()` | `string` | Serialized URL, including query edits and fragment. |

`host` includes IPv6 brackets; URLs without a host return an empty string.
Serialization preserves present but empty query and fragment delimiters.

## Query parameters

`query` is a mutable ordered collection attached to the URL. Keys and values are
decoded, and repeated keys stay distinct.

| Member | Returns | Effect |
|---|---|---|
| `length` | `int` | Number of pairs, including duplicates. |
| `append(key, value)` | `void` | Adds a pair after existing ones. |
| `get(key)` | `Option<string>` | First value for `key`, or `None`. |
| `get_all(key)` | `string[]` | Copy of every value for `key` in order. |
| `set(key, value)` | `void` | Replaces the first occurrence and removes later duplicates; appends if absent. |
| `remove(key)` | `void` | Removes every pair with `key`. |
| `clear()` | `void` | Removes all pairs and the `?` delimiter. |

Existing query encoding is preserved until mutation. Mutations serialize decoded
pairs using form encoding: spaces become `+`, literal plus signs become `%2b`,
and empty values retain `=`. URI-component codecs in `system.encoding` have
different plus-sign rules. URL parsing follows the host URL Standard's recovery
rules; it does not impose strict UTF-8 decoding on query values.

Opaque paths preserve spaces inside the path. A final path space before a query
or fragment is serialized as `%20` consistently across native and JavaScript hosts.
