---
title: Middleware and CORS
summary: Wrap every request with shared behavior.
icon: material/layers-plus
---

# Middleware and CORS

Middleware receives a context and a continuation. Register global layers explicitly on an app.

```dream
import system;
import system.webapi;

async fun request_id(ctx: RequestContext, next: Next): HttpOutgoing {
    ctx.set("request_id", ctx.incoming.header("X-Request-ID").unwrap_or("local"));
    let response = next.call().await;
    return response.set_header("X-Request-ID", ctx.get("request_id").unwrap_or(""));
}

@router("")
class Routes {
    @get("/hello")
    public static fun hello(): string {
        return "hello";
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.use(Middleware(request_id)).unwrap();
    app.use(
        CORS(
            CorsOptions(
                allow_origins: "https://example.com",
                allow_methods: "GET,POST",
                allow_headers: "Authorization,Content-Type"
            )
        )
    ).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

The first registered layer is outermost. Requests pass through app layers, router `@use(handler)` layers, route `@use(handler)` layers, dependency extraction, and the handler. Responses return in reverse order. Repeated `@use` attributes preserve source order.

A layer may return early, for example to reject a request or answer a preflight, without calling `next`. Call `next.call().await` once to continue. App middleware also surrounds documentation and unmatched-route responses.

`CorsOptions` controls allowed origins, methods, request headers, exposed headers, credential handling, and preflight cache duration. The current API uses comma-separated strings. With credentials enabled, the response echoes an allowed origin rather than combining credentials with a wildcard origin. Include custom response headers in `expose_headers` when browser code must read them.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
