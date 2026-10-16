# Http Types

**Import:** `import system.net;`

## `public class HttpError : Error`

A transport, validation, cancellation, deadline, or capability failure; HTTP statuses are not errors.

```dream
public class HttpError : Error
```

## `public constructor`

Creates a failure with a stable machine code and a human-readable description.

```dream
public constructor(code: string, message: string)
```

## `public fun code`

Stable category such as `EINVAL`, `ETIMEOUT`, `ECANCELLED`, `ECLOSED`, or `ENOTSUP`.

```dream
public fun code(): string
```

## `public fun message`

Human-readable failure detail; transport adapters omit request URLs from error messages.

```dream
public fun message(): string
```

## `public class HttpOptions`

Request deadline and connection/redirect controls; unsupported host controls return `ENOTSUP`.

```dream
public class HttpOptions
```

## `public timeout_ms: int`

Total request deadline including response-body consumption; zero disables the deadline.

```dream
public timeout_ms: int
```

## `public connect_timeout_ms: int`

Native connection establishment timeout; zero uses the host default. Fetch requires zero.

```dream
public connect_timeout_ms: int
```

## `public max_redirects: int`

Maximum redirects; -1 uses the host default, zero exposes redirect responses on native/Node. Browsers require -1 because redirect counts and responses are not reliably observable.

```dream
public max_redirects: int
```

## `public constructor`

Creates options; negative deadlines or redirect limits below -1 are invalid at send time.

```dream
public constructor(timeout_ms: int = 30000, max_redirects: int = -1, connect_timeout_ms: int = 0)
```

## `public class HttpHeaders`

Case-insensitive header names with ordered repeated values. Validation occurs when sending.

```dream
public class HttpHeaders
```

## `public constructor`

Creates an empty header collection.

```dream
public constructor()
```

## `public get length`

Number of stored header pairs, including repeated names.

```dream
public get length(): int
```

## `public fun append`

Appends a value without replacing existing values for the same name.

```dream
public fun append(name: string, value: string): void
```

## `public fun set`

Replaces all existing values for `name` with one value at the end of the collection.

```dream
public fun set(name: string, value: string): void
```

## `public fun get`

Returns the first matching value, or `None` when absent.

```dream
public fun get(name: string): Option<string>
```

## `public fun get_all`

Copies every matching value in stored order; JS Fetch may already have combined repeated headers.

```dream
public fun get_all(name: string): string[]
```

## `public fun remove`

Removes all values with the given case-insensitive name.

```dream
public fun remove(name: string): void
```

## `public fun to_json`

Serializes header pairs in order, preserving repeated names.

```dream
public fun to_json(): JsonValue
```

## `public class HttpRequest`

Mutable request description; sending snapshots URL, headers, options, and body bytes.

```dream
public class HttpRequest
```

## `public method: string`

HTTP method, restricted to GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS on all hosts.

```dream
public method: string
```

## `public url: Url`

Absolute http/https destination; credentials in URLs are rejected.

```dream
public url: Url
```

## `public headers: HttpHeaders`

Request headers; browsers reject forbidden names instead of silently ignoring them.

```dream
public headers: HttpHeaders
```

## `public body: byte[]`

Byte body copied for each send; GET and HEAD require an empty body.

```dream
public body: byte[]
```

## `public constructor`

Creates a request with empty headers and body.

```dream
public constructor(method: string, url: Url)
```
