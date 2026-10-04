# Url

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Url`

Parsed URL components.

```dream
public class Url
```

## `scheme: string`

Scheme without trailing `://` (e.g. `https`); empty when absent.

```dream
public scheme: string
```

## `host: string`

Host name or address (without port).

```dream
public host: string
```

## `port: Option<int>`

Explicit port when present in the source text.

```dream
public port: Option<int>
```

## `path: string`

Path beginning with `/` (defaults to `/`).

```dream
public path: string
```

## `query: string`

Query string without the leading `?`.

```dream
public query: string
```

## `fragment: string`

Fragment without the leading `#`.

```dream
public fragment: string
```

## `constructor`

Builds a URL from already-split components.

```dream
public constructor( scheme: string, host: string, port: Option<int>, path: string, query: string, fragment: string )
```

## `parse`

Parses an absolute or scheme-relative URL; rejects empty input.

```dream
public static fun parse(text: string): Result<Url, ParseError>
```

## `to_string`

Reconstructs the URL string from components.

```dream
public override fun to_string(): string
```

## `with_path`

Returns a copy with `path` replaced.

```dream
public fun with_path(path: string): Url
```

## `with_query`

```dream
public fun with_query(query: string): Url
```

## `with_host`

```dream
public fun with_host(host: string): Url
```

## `with_fragment`

```dream
public fun with_fragment(fragment: string): Url
```

## `query_params`

```dream
public fun query_params(): Map<string, string>
```

## `join`

Resolves `relative` against this URL (absolute http(s) URLs replace entirely).

```dream
public fun join(relative: string): Result<Url, ParseError>
```
