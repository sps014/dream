# HttpRequest

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `HttpRequest`

Mutable request description; sending snapshots URL, headers, options, and body bytes.

```dream
public class HttpRequest
```

## `method`

HTTP method, restricted to GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS on all hosts.

```dream
public method: HttpMethod
```

## `url`

Absolute http/https destination; credentials in URLs are rejected.

```dream
public url: Url
```

## `headers`

Request headers; browsers reject forbidden names instead of silently ignoring them.

```dream
public headers: HttpHeaders
```

## `body`

Byte body copied for each send; GET and HEAD require an empty body.

```dream
public body: byte[]
```

## `constructor`

Creates a request with empty headers and body.

```dream
public constructor(method: HttpMethod, url: Url)
```

## `set_text`

Replaces the body with UTF-8 bytes; adds no content-type header implicitly.

```dream
public fun set_text(text: string): void
```
