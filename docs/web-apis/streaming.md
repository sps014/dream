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

Try `curl -N http://127.0.0.1:8000/events`. `SseEvent` carries data and optional event name, ID, and retry duration. Newlines in data become separate SSE data lines. Event responses use `text/event-stream`.

`Iterable<string>`, `AsyncIterable<string>`, and byte-array variants stream chunks. Strings default to text; bytes default to `application/octet-stream`. `@produces("content/type")` overrides chunk content types. SSE keeps its required content type.

The dispatcher writes incrementally with backpressure. Client disconnect cancels the handler at a suspension point and closes iterator resources. Request dependency cleanup runs after streaming stops, so a resource needed to enumerate the stream remains alive for its full lifetime.

Once response headers or chunks have been sent, later errors cannot replace them with a new HTTP error status. Design the stream protocol to communicate application-level failures when needed.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
