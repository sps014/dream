# EventStream

**Import:** `import system.webapi;`

Read the [usage guide](../stdlib/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class EventStream`

Server-sent events (`text/event-stream`). Returning this from a `@get` handler starts a chunked response instead of a oneshot `httpServerRespond`.

```dream
public class EventStream
```

## `constructor`

```dream
public constructor(req: HttpIncoming)
```

## `start`

```dream
public fun start(): void
```

## `send`

```dream
public async fun send(event: string, data: string, token: Option<CancellationToken> = Option.None): void
```

## `send_data`

```dream
public async fun send_data(data: string, token: Option<CancellationToken> = Option.None): void
```

## `end`

```dream
public fun end(): void
```
