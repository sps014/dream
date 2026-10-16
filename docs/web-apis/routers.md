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

For multiple files, put a public router class in `users.dream`, import that file from the application entry point, and call `app.include_router(Users.router()).unwrap()`. Put request/response models and account repositories in their own files. The [complete example](example.md) shows the exact layout.

Only explicitly mounted routers are served or documented. Duplicate methods and paths within one router are compile-time errors. Duplicate routes introduced by mounting are startup errors. Keep endpoint names and paths stable when clients use the generated OpenAPI operation IDs.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
