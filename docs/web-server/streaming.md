---
title: Streaming and SSE
summary: Stream items and server-sent events without buffering.
icon: material/broadcast
---

# Streaming and server-sent events

Yield items instead of building a complete response in memory.

```dream
import system;
import system.webapi;

@router("")
class Routes {
    @get("/events")
    public static async fun events(): AsyncIterable<SseEvent> {
        yield SseEvent.data("connected");
        Time.sleep(100).await;
        yield SseEvent("ready", Option.Some("update"), Option.Some("2"), Option.Some(1000));
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

Try `curl -N http://127.0.0.1:8000/events`.

## Stream types

A streaming handler returns `Iterable<T>` (with `yield`) or `AsyncIterable<T>` (an `async` function with `yield`). Each yielded item is sent as soon as it is produced.

| Item type `T` | Default `Content-Type` | `@produces` |
| --- | --- | --- |
| `SseEvent` | `text/event-stream` | Ignored; SSE always uses its own type. |
| `string` | `text/plain; charset=utf-8` | Replaces the content type. |
| `byte[]` | `application/octet-stream` | Replaces the content type. |

Any other item type is a compile-time error. Write `@produces("application/x-ndjson")` on the handler to send newline-delimited JSON strings, for example.

## Server-sent events

| `SseEvent` field | Type | Sent as |
| --- | --- | --- |
| `data` | `string` | One `data:` line per line of text. |
| `event` | `Option<string>` | `event:`, the event type the browser listens for. |
| `id` | `Option<string>` | `id:`, which the browser sends back as `Last-Event-ID` when it reconnects. |
| `retry` | `Option<int>` | `retry:`, the reconnection delay in milliseconds. |

`SseEvent.data(text)` creates an event with only data. The full constructor takes the fields in the order shown.

The dispatcher writes incrementally with backpressure. Client disconnect cancels the handler at a suspension point and closes iterator resources. Request dependency cleanup runs after streaming stops, so a resource needed to enumerate the stream remains alive for its full lifetime.

Once response headers or chunks have been sent, later errors cannot replace them with a new HTTP error status. Design the stream protocol to communicate application-level failures when needed.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
