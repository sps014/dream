---
title: Resources and overrides
summary: Pair dependencies with cleanup and override them in tests.
icon: material/recycle
---

# Resource cleanup and dependency overrides

A resource provider pairs a typed value with an asynchronous cleanup callback.

```dream
import system;
import system.webapi;

fun connection(): DependencyResource<string> {
    return DependencyResource<string>(
        "connection",
        async (token: CancellationToken) => {
            System.println("released");
        }
    );
}

class Overrides {
    @dependency_override(connection)
    public static fun fake_connection(): string {
        return "test connection";
    }
}

@router("")
class Routes {
    @get("/resource")
    public static fun resource(@dep(connection) value: string): string {
        return value;
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.override_dependencies(Overrides.dependencies()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

| Name | Use it to |
| --- | --- |
| `DependencyResource<T>(value, cleanup)` | Return `value` to the handler and run `async cleanup(token)` after the request finishes. |
| `@dependency_override(original)` | Mark a static method as the replacement for the provider `original`. |
| `ClassName.dependencies()` | Get the generated `DependencyOverrides` for every `@dependency_override` in that class. |
| `app.override_dependencies(overrides)` | Install those replacements on one app before it starts. |
| `ctx.add_cleanup(cleanup)` | Register cleanup by hand from code that has the `RequestContext`. |

Without the override, `connection` registers a release operation immediately after acquisition. Cleanup runs once, in reverse acquisition order, after the response or stream finishes. It also runs when later dependency extraction fails or a client disconnect cancels the handler. A fresh cleanup token is separate from request cancellation. Pass that token to cleanup I/O rather than reusing the cancelled request token. Process aborts do not run asynchronous cleanup.

`Overrides.dependencies()` is generated from static methods carrying `@dependency_override(original)`. Install it on an app before startup. A replacement may have its own extracted parameters and dependencies, and must produce the original provider's value type. It may add or remove resource acquisition while preserving that value type.

Overrides belong to the app, so tests can replace a provider without changing another running app. A request still caches the resolved provider identity. Replacing a provider with an incompatible type, an unresolved function, or a cyclic dependency is a compile-time error.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
