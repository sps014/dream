---
title: URLs
summary: Parse URLs and work with query parameters.
icon: material/link-variant
api: reference/api/system-net.md
---

# URLs and query parameters

Import `system.net` to parse an absolute URL and resolve a relative one. Parsing follows the WHATWG URL Standard on every target and returns `Result<Url, ParseError>`.

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

Normalization:

- lowercases the scheme and host
- removes the default port and `.` / `..` segments
- canonicalizes IPv4 and IPv6 (`http://0x7f.1/` becomes `http://127.0.0.1/`)
- turns `\` into `/` for `http`, `https`, `ws`, `wss`, `ftp`, and `file`
- percent-encodes spaces and non-ASCII text in the path, query, and fragment

`/v1/` is a directory base, and `/v1` replaces its last segment with the relative reference. An absolute reference replaces the whole base.

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

Existing query text stays until you change the query. After a change, spaces become `+`, a literal `+` becomes `%2b`, and an empty value keeps `=`.

`system.encoding` treats `+` differently from query form encoding. A bad percent escape stays as written, and invalid UTF-8 in a decoded value becomes U+FFFD instead of failing the parse.

Opaque paths preserve spaces inside the path. A final path space before a query
or fragment is serialized as `%20`.

## Internationalized hostnames

A hostname with non-ASCII letters becomes ASCII (Punycode). `https://bücher.example/` has host `xn--bcher-kva.example`.

Native, Node, and the browser convert those names. Standalone WASI (`wasm32-wasip2` and `wasm32-wasip3`) does not, so write the `xn--` form or `Url.parse` returns `ParseError`.
