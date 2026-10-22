# Url

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Url`

An absolute URL parsed and normalized as the WHATWG URL Standard describes: the scheme and host are lowercased, default ports and `.`/`..` path segments are removed, internationalized hostnames become Punycode, IPv4 and IPv6 hosts are canonicalized, and characters outside each component's allowed set are percent-encoded. Query parameters keep their original encoding until modified.

```dream
public class Url
```

## `parse`

Parses and normalizes an absolute URL. Relative references and malformed input, such as an invalid host or a port above 65535, return `ParseError`.

```dream
public static fun parse(input: string): Result<Url, ParseError>
```

## `resolve`

Resolves a reference against this URL; an absolute reference replaces it entirely. A trailing slash in this URL's path determines whether its last segment is a directory. URLs with an opaque path, such as `mailto:`, only resolve fragment-only references.

```dream
public fun resolve(reference: string): Result<Url, ParseError>
```

## `scheme`

Lowercase scheme without the colon delimiter.

```dream
public get scheme(): string
```

## `host`

Canonical hostname, with IPv6 brackets when present; empty for URLs without a host.

```dream
public get host(): string
```

## `path`

Encoded path component; percent escapes remain encoded.

```dream
public get path(): string
```

## `query`

Ordered, mutable query parameters shared with this URL; repeated keys remain distinct.

```dream
public get query(): UrlQuery
```

## `to_string`

Serializes the normalized URL, including query edits and any fragment delimiter.

```dream
public override fun to_string(): string
```
