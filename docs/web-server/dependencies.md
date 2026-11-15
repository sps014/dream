---
title: Dependencies
summary: "Inject provider values into handlers with `@dep`."
icon: material/needle
---

# Dependencies

Declare a provider function and inject its value with `@dep(provider)`.

```dream
import system;
import system.webapi;

fun request_key(@header("X-Key") key: string): Result<string, HttpStatus> {
    if key.is_empty() {
        return Result.Err(HttpStatus.unauthorized());
    }
    return Result.Ok(key);
}

@router("")
class Routes {
    @get("/key")
    public static fun key(@dep(request_key) value: string): string {
        return value;
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

## `@dep` arguments

| Argument | Default | Meaning |
| --- | --- | --- |
| `provider` | Required | The function whose result fills this parameter. |
| `cache` | `true` | Reuse the provider's result for the rest of this request. `@dep(provider, false)` runs it again for this parameter. |

## What a provider can return

| Provider returns | The parameter receives | On failure |
| --- | --- | --- |
| `T` | `T` | — |
| `Result<T, HttpStatus>` | `T` | The request stops with that status. |
| `DependencyResource<T>` | `T`, with cleanup registered | — (see [resources](resources.md)) |

Providers may be synchronous or `async`, and their parameters use the same sources as a handler (`@path`, `@query`, `@header`, `@cookie`, `@body`, `HttpIncoming`, `RequestContext`, and other `@dep` values). A missing `@header` inside a provider returns 401 instead of 422, and unknown providers and cycles are compile-time errors.

Each request caches a provider by its declaration, so two parameters that use it share one result, including through other providers. Use `@dep(provider, false)` when that injection must run on its own; the cache ends with the request.

A `Result<T, HttpStatus>` failure stops the endpoint with that status, and a resource provider registers cleanup as soon as acquisition succeeds. Failure, cancellation, and overrides are covered in the [resource guide](resources.md).

Use dependencies for values an endpoint needs, such as identity, a repository, or a request-scoped resource. Use middleware for behavior surrounding requests and responses.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
