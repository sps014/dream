# Sse Event

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../stdlib/webapi.md).

## `public class SseEvent`

One server-sent event. Newlines in data are emitted as separate data lines.

```dream
public class SseEvent
```

## `public data: string`

```dream
public data: string
```

## `public event: Option<string>`

```dream
public event: Option<string>
```

## `public id: Option<string>`

```dream
public id: Option<string>
```

## `public retry: Option<int>`

```dream
public retry: Option<int>
```

## `public constructor`

```dream
public constructor(data: string, event: Option<string> = Option.None, id: Option<string> = Option.None, retry: Option<int> = Option.None)
```

## `public static fun data`

Creates an event with only a data payload.

```dream
public static fun data(text: string): SseEvent
```
