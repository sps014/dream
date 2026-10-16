# Your first application

Save this as `app.dream`, then run `dream run app.dream`.

```dream
import system;
import system.webapi;

@router("")
class Routes {
    @get("/hello/{name}")
    public static fun hello(@path name: string): string {
        return "Hello, " + name;
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

Open `http://127.0.0.1:8000/hello/Dream`. The string response uses `text/plain`. Visit `/docs` for Swagger UI, `/redoc` for ReDoc, and `/openapi.json` for the generated document.

`@router` creates a reusable group; `Routes.router()` is generated at compile time. `include_router` makes the group part of this app. Configuration methods return `Result`, so handle or unwrap failures while building the application.

Handlers may be synchronous or asynchronous. Use [JSON models](validation.md) for typed request and response bodies. Keep handlers static; services and per-request values enter through [dependencies](dependencies.md).

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
