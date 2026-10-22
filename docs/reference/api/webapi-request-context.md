# RequestContext

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `RequestContext`

Per-request bag shared by middleware and `@dep` functions.

```dream
public class RequestContext
```

## `incoming`

Request received from the client.

```dream
public incoming: HttpIncoming
```

## `app_state`

Application values shared by requests.

```dream
public app_state: Map<string, string>
```

## `state`

Values stored for this request by handlers and middleware.

```dream
public state: Map<string, string>
```

## `cancellation_token`

Cancellation signal for the current request, when available.

```dream
public cancellation_token: Option<CancellationToken>
```

## `constructor`

Creates a `RequestContext` with `incoming`.

```dream
public constructor(incoming: HttpIncoming)
```

## `add_cleanup`

Registers request cleanup, run once in reverse acquisition order outside request cancellation.

```dream
public fun add_cleanup(cleanup: fun(CancellationToken): Future<void>): void
```

## `set`

Stores `value` under `key` for the rest of this request, replacing any earlier value.

```dream
public fun set(key: string, value: string): void
```

## `get`

Returns the value stored under `key` for this request, or `None` when nothing was stored.

```dream
public fun get(key: string): Option<string>
```
