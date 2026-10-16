---
title: Routers and multiple files
summary: Group handlers under a prefix and mount them explicitly.
icon: material/router
---

# Routers and multiple files

A router class groups static handlers under a path prefix. Mounting adds an optional application-specific prefix.

```dream
import system;
import system.webapi;

@router("/users")
class Routes {
    @get("/{id}")
    public static fun user(@path id: int): string {
        return id.to_string();
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router(), "/v1").unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

This app exposes `/v1/users/{id}`. Mount the same descriptor on another app with `/v2` without changing the router.

## Route attributes

| Attribute | Put on | Effect |
| --- | --- | --- |
| `@router(prefix)` | Class | Groups the class's static handlers under `prefix` and generates `ClassName.router()`. Use `""` for no prefix. |
| `@get(path)` | Static method | Handles `GET` requests to `path`. |
| `@post(path)` | Static method | Handles `POST` requests. |
| `@put(path)` | Static method | Handles `PUT` requests. |
| `@patch(path)` | Static method | Handles `PATCH` requests. |
| `@delete(path)` | Static method | Handles `DELETE` requests. |
| `@head(path)` | Static method | Handles `HEAD` requests. |
| `@options(path)` | Static method | Handles `OPTIONS` requests. |
| `@websocket(path)` | Static method | Upgrades the request to a [WebSocket](websockets.md). |
| `@use(middleware)` | Class or static method | Runs a [middleware](middleware.md) function around the group or route; repeatable. |

Route paths are relative to the router prefix. Write a named segment as `{name}` and bind it with a parameter of the same name. Other per-route attributes are covered in their guides: [`@operation` and `@response`](openapi.md#operation-attributes), [`@requires_scopes`](authorization.md), and [`@produces`](streaming.md).

## Mounting

| `WebApp` method | Effect |
| --- | --- |
| `include_router(router, prefix = "")` | Serves and documents every route in `router`, with `prefix` added in front of the router's own prefix. |

For multiple files, put a public router class in `users.dream`, import that file from the application entry point, and call `app.include_router(Users.router()).unwrap()`. Put request/response models and account repositories in their own files. The [complete example](example.md) shows the exact layout.

Only explicitly mounted routers are served or documented. Duplicate methods and paths within one router are compile-time errors. Duplicate routes introduced by mounting are startup errors. Keep endpoint names and paths stable when clients use the generated OpenAPI operation IDs.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
