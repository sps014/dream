---
title: Configuration
summary: "Configure an app with a `WebAppOptions` snapshot."
icon: material/cog-outline
---

# Application configuration

Pass a `WebAppOptions` snapshot to the app constructor. Changes to the original options afterward do not change the app.

```dream
import system;
import system.webapi;

async fun main(): void {
    let options = WebAppOptions(
        title: "Orders API",
        version: "1.0.0",
        docs_url: "/documentation",
        redoc_url: "",
        description: "Manage orders"
    );
    options.swagger = SwaggerOptions(persist_authorization: false, deep_linking: true);
    let app = WebApp(options);
    let port = app.listen("127.0.0.1", 0).await.unwrap();
    System.println("Listening on " + port.to_string());
    app.shutdown();
    app.wait().await;
}
```

`title` defaults to `Dream API`, `version` to `0.1.0`, and the documentation URLs to `/docs`, `/openapi.json`, and `/redoc`. An empty URL disables its endpoint. TLS certificate and key paths default to empty strings.

`listen` returns the bound port; zero requests an ephemeral port. `run` listens and waits. Both accept an optional cancellation token. Call `shutdown` followed by `wait` to release the listener and finish cleanup.

Mount routers, install middleware and overrides, and register lifespan hooks before startup. Startup freezes configuration; later configuration calls return an error. Duplicate routes and documentation URL collisions fail before accepting traffic. Treat each app as one server lifetime: construct another instance to run a fresh server.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
