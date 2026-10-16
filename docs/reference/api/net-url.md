# Url

**Import:** `import system.net;`

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

## `UrlQuery`

Ordered form-decoded URL query parameters. Mutations use form encoding (`+` for space).

```dream
public class UrlQuery
```

## `length`

Number of key/value pairs, including repeated keys and empty values.

```dream
public get length(): int
```

## `append`

Appends a decoded key/value pair after existing parameters.

```dream
public fun append(key: string, value: string): void
```

## `get`

Returns the first decoded value for `key`, or `None` if the key is absent.

```dream
public fun get(key: string): Option<string>
```

## `get_all`

Copies all decoded values for `key` in their original order.

```dream
public fun get_all(key: string): string[]
```

## `set`

Replaces the first occurrence in place and removes later duplicates; appends if absent.

```dream
public fun set(key: string, value: string): void
```

## `remove`

Removes every occurrence of `key`, preserving the order of remaining pairs.

```dream
public fun remove(key: string): void
```

## `clear`

Removes all parameters and the query delimiter from the URL.

```dream
public fun clear(): void
```
