# HttpOptions

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
