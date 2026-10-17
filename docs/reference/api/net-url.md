# Url

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Url`

A normalized absolute URL parsed using the host URL Standard implementation. Query parameters preserve their original encoding until modified.

```dream
public class Url
```

## `parse`

Parses and normalizes an absolute URL; relative or malformed input returns `ParseError`.

```dream
public static fun parse(input: string): Result<Url, ParseError>
```

## `resolve`

Resolves a relative reference against this URL; an absolute reference replaces the base. A trailing slash in the base determines whether its last path component is a directory.

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
