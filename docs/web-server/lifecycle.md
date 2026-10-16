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

Startup hooks run in registration order before accepting traffic. A successful hook activates its matching shutdown hook. If a later startup hook or listener bind fails, completed hooks unwind in reverse order; the failed hook must release any partially acquired resources itself.

`shutdown` stops acceptance and cancels pending transport operations. `wait` drains active requests and their cleanup before invoking application shutdown hooks. Call both during controlled termination. A trailing cancellation token on `listen`, `run`, or `wait` participates in structured cancellation.

`app.state` is a string map shared by requests to that app; handlers access it as `ctx.app_state`. `ctx.state`, `ctx.get`, and `ctx.set` are request-local. Keep typed resources in dependencies or service objects rather than serializing handles into these maps.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
