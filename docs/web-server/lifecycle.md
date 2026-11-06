---
title: Lifecycle and state
summary: Run startup and shutdown work and hold application state.
icon: material/power
---

# Lifecycle and application state

A web service shares its process lifetime with every other Dream program: wrap `main` in `Application.run` from `system.app`. Startup code goes in the `start` function, and each resource registers its release with `Application.on_stop` right after it is acquired. Ctrl+C and `SIGTERM` then stop the server gracefully.

```dream
import system;
import system.app;
import system.webapi;

@router("")
class Routes {
    @get("/state")
    public static fun state(ctx: RequestContext): string {
        return ctx.app_state.get("service").unwrap_or("");
    }
}

async fun main(): int {
    return Application.run(async () => {
        let app = WebApp();
        app.include_router(Routes.router()).unwrap();
        app.state["service"] = "ready";
        Application.on_stop(async () => {
            app.state.clear();
        });

        app.run("127.0.0.1", 8000).await?;
        return Result.Ok(Unit());
    }).await;
}
```

When the process is asked to stop, `Application.stopping` is cancelled. `run` without a token watches that token, so it stops accepting connections, lets running requests finish, and returns. `Application.run` then runs the `on_stop` cleanups, newest first, and returns the exit code for `main`: `130` after Ctrl+C and `143` after `SIGTERM`. If `start` returns an error, for example because `run` could not bind the port, the error is printed, the cleanups registered so far run, and the exit code is `1`.

To start the server without blocking `start`, call `listen` and register the drain as a cleanup:

```dream
let port = app.listen("127.0.0.1", 0).await?;
Application.on_stop(async () => {
    app.shutdown();
    app.wait().await;
});
```

| `WebApp` member | Effect |
| --- | --- |
| `state` | A `Map<string, string>` shared by every request; handlers see it as `ctx.app_state`. |
| `run(host, port, token?)` | Listens, then waits until the server stops. Without a token it stops when `Application.stopping` is cancelled. |
| `shutdown()` | Stops accepting connections and cancels pending transport work. |
| `wait(token?)` | Waits for active requests and their cleanup, then releases the listener. |

`shutdown` stops acceptance and cancels pending transport operations. `wait` drains active requests and their cleanup. Call both during controlled termination. A trailing cancellation token on `listen`, `run`, or `wait` participates in structured cancellation. See [Application lifetime](../library/system/app.md) for the shutdown timeout and the other process events.

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
