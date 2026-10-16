---
title: Lifecycle and state
summary: Run startup and shutdown work and hold application state.
icon: material/power
---

# Lifecycle and application state

Register paired startup and shutdown operations before listening.

```dream
import system;
import system.webapi;

@router("")
class Routes {
    @get("/state")
    public static fun state(ctx: RequestContext): string {
        return ctx.app_state.get("service").unwrap_or("");
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.lifespan(
        async (app: WebApp) => {
            app.state.set("service", "ready");
            return Result.Ok(true);
        },
        async (app: WebApp) => {
            app.state.clear();
        }
    ).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

| `WebApp` member | Effect |
| --- | --- |
| `lifespan(start, stop)` | Registers a pair: `start(app)` returns `Result<bool, IoError>` and runs at startup; `stop(app)` runs at shutdown only if `start` succeeded. |
| `state` | A `Map<string, string>` shared by every request; handlers see it as `ctx.app_state`. |
| `shutdown()` | Stops accepting connections and cancels pending transport work. |
| `wait(token?)` | Waits for active requests and their cleanup, then runs the `stop` hooks. |

Startup hooks run in registration order before accepting traffic. A successful hook activates its matching shutdown hook. If a later startup hook or listener bind fails, completed hooks unwind in reverse order; the failed hook must release any partially acquired resources itself.

`shutdown` stops acceptance and cancels pending transport operations. `wait` drains active requests and their cleanup before invoking application shutdown hooks. Call both during controlled termination. A trailing cancellation token on `listen`, `run`, or `wait` participates in structured cancellation.

## Request context

A handler, provider, or middleware that declares a `RequestContext` parameter receives the current one.

| `RequestContext` member | Type | Scope |
| --- | --- | --- |
| `incoming` | `HttpIncoming` | The request: `method`, `path`, `query`, `headers`, plus `header(name)`, `query_param(name)`, and `cookie(name)`. |
| `app_state` | `Map<string, string>` | Shared by every request to this app; the same map as `app.state`. |
| `state` | `Map<string, string>` | This request only. |
| `get(key)` / `set(key, value)` | `Option<string>` / `void` | Shorthand for reading and writing `state`. |
| `cancellation_token` | `Option<CancellationToken>` | Cancelled when the client disconnects. |
| `add_cleanup(cleanup)` | `void` | Runs `async cleanup(token)` after the response finishes. |

Keep typed resources in dependencies or service objects rather than serializing handles into these maps.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
