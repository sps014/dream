# Sse Event

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../../web-apis/index.md).

## `SseEvent`

One server-sent event. Newlines in data are emitted as separate data lines.

```dream
public class SseEvent
```

## `data`

Text payload of the event.

```dream
public data: string
```

## `event`

Optional event type sent to the client.

```dream
public event: Option<string>
```

## `id`

Optional event identifier used when reconnecting.

```dream
public id: Option<string>
```

## `retry`

Optional reconnection delay in milliseconds.

```dream
public retry: Option<int>
```

## `constructor`

Creates a `SseEvent` with `data`, `event`, `id`, `retry`.

```dream
public constructor(
    data: string,
    event: Option<string> = Option.None,
    id: Option<string> = Option.None,
    retry: Option<int> = Option.None
)
```

## `data`

Creates an event with only a data payload.

```dream
public static fun data(text: string): SseEvent
```
