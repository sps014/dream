---
title: First application
summary: Write and run a minimal HTTP service.
icon: material/flag-checkered
---

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

Open `http://127.0.0.1:8000/hello/Dream` for a `text/plain` response. `/docs` is Swagger UI, `/redoc` is ReDoc, and `/openapi.json` is the generated document.

`@router` creates a reusable group, and `include_router` adds `Routes.router()` to this app. Configuration methods return `Result`, so handle or unwrap failures while you build the application.

Handlers may be synchronous or asynchronous, and they stay static. Use [JSON models](validation.md) for typed bodies, and pass services and per-request values through [dependencies](dependencies.md).

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
