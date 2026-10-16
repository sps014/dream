# Http Types

**Import:** `import system.net;`

## `HttpError`

A transport, validation, cancellation, deadline, or capability failure; HTTP statuses are not errors.

```dream
public class HttpError: Error
```

## `constructor`

Creates a failure with a stable machine code and a human-readable description.

```dream
public constructor(code: string, message: string)
```

## `code`

Stable category such as `EINVAL`, `ETIMEOUT`, `ECANCELLED`, `ECLOSED`, or `ENOTSUP`.

```dream
public fun code(): string
```

## `message`

Human-readable failure detail; transport adapters omit request URLs from error messages.

```dream
public fun message(): string
```

## `HttpOptions`

Request deadline and connection/redirect controls; unsupported host controls return `ENOTSUP`.

```dream
public class HttpOptions
```

## `timeout_ms`

Total request deadline including response-body consumption; zero disables the deadline.

```dream
public timeout_ms: int
```

## `connect_timeout_ms`

Native connection establishment timeout; zero uses the host default. Fetch requires zero.

```dream
public connect_timeout_ms: int
```

## `max_redirects`

Maximum redirects; -1 uses the host default, zero exposes redirect responses on native/Node. Browsers require -1 because redirect counts and responses are not reliably observable.

```dream
public max_redirects: int
```

## `constructor`

Creates options; negative deadlines or redirect limits below -1 are invalid at send time.

```dream
public constructor(timeout_ms: int = 30000, max_redirects: int = -1, connect_timeout_ms: int = 0)
```

## `HttpHeaders`

Case-insensitive header names with ordered repeated values. Validation occurs when sending.

```dream
public class HttpHeaders
```

## `constructor`

Creates an empty header collection.

```dream
public constructor()
```

## `length`

Number of stored header pairs, including repeated names.

```dream
public get length(): int
```

## `append`

Appends a value without replacing existing values for the same name.

```dream
public fun append(name: string, value: string): void
```

## `set`

Replaces all existing values for `name` with one value at the end of the collection.

```dream
public fun set(name: string, value: string): void
```

## `get`

Returns the first matching value, or `None` when absent.

```dream
public fun get(name: string): Option<string>
```

## `get_all`

Copies every matching value in stored order; JS Fetch may already have combined repeated headers.

```dream
public fun get_all(name: string): string[]
```

## `remove`

Removes all values with the given case-insensitive name.

```dream
public fun remove(name: string): void
```

## `to_json`

Serializes header pairs in order, preserving repeated names.

```dream
public fun to_json(): JsonValue
```

## `HttpRequest`

Mutable request description; sending snapshots URL, headers, options, and body bytes.

```dream
public class HttpRequest
```

## `method`

HTTP method, restricted to GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS on all hosts.

```dream
public method: string
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
public constructor(method: string, url: Url)
```
