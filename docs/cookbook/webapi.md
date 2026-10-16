# Serve a native JSON API

Create a route function and a native server entry point:

```dream
import system.webapi;
import system.json;

@json
class Item {
    public id: int; public name: string;
    public constructor(id: int, name: string) { this.id = id; this.name = name; }
}

@get("/items/{id}")
fun item(@path id: int, @query name: Option<string>): Item {
    return Item(id, name.unwrap_or("Dream"));
}

@post("/items")
fun create(@body item: Item): Item { return item; }

async fun main(): void {
    WebApp.run("127.0.0.1", 8000).await.unwrap();
}
```

Save as `app.dream` and run `dream run app.dream`. Visit
`http://127.0.0.1:8000/items/1?name=Example` for JSON or
`http://127.0.0.1:8000/docs` to try both endpoints in Swagger UI.
The viewer loads its scripts from a CDN.

Use `WebApp.listen("127.0.0.1", 0)` when another part of the program needs the
bound port. Call `WebApp.shutdown()` and then `WebApp.wait().await` when finished.
For TLS and configurable docs endpoints, use `WebAppOptions` with `run_with` or
`listen_with`. This server is native only; the `system.net.WebSocket` client also
works in Node 22+ and browsers.

The [Web API guide](../reference/stdlib/webapi.md) covers dependencies, middleware,
CORS, multipart uploads, streaming and WebSocket routes.
