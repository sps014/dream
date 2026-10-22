---
title: Responses and errors
summary: Return JSON models or explicit status codes and headers.
icon: material/export
---

# Responses and errors

Return a model for JSON or `HttpOutgoing` when you need explicit headers and status.

```dream
import system;
import system.webapi;

@router("")
class Routes {
    @get("/hello")
    public static fun hello(): HttpOutgoing {
        return HttpOutgoing.text("hello").set_header("X-Service", "dream");
    }

    @get("/private")
    public static fun private_page(): Result<string, HttpStatus> {
        return Result.Err(HttpStatus.unauthorized());
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

## Return types

| Handler returns | Response |
| --- | --- |
| `string` | 200, `text/plain` |
| `byte[]` | 200, `application/octet-stream` |
| An `@json` model | 200, `application/json` |
| `void` | 204 with no body |
| `HttpOutgoing` | Exactly the status, headers, and body you built |
| `HttpStatus` | That status code and reason |
| `Result<T, HttpStatus>` | `T` as above on `Ok`; the given status on `Err` |
| `Result<T, E>` for another `E` | `T` on `Ok`; 500 with `{"detail": "..."}` on `Err` |
| `Iterable<T>` or `AsyncIterable<T>` | A [streamed response](streaming.md) |

`@operation` with a status, such as `@operation("Create item", "Creates one item", "items", 201)`, replaces the default 200 or 204. Return an explicit `HttpOutgoing` when the status depends on the request.

Prefer intentional client-safe errors. Do not put database credentials or internal exceptions into returned strings, because a `Result<T, string>` error message is sent to the client.

## Building responses

| `HttpOutgoing` factory | Response |
| --- | --- |
| `text(body, status = 200)` | Plain text |
| `html(body, status = 200)` | HTML |
| `json_text(body, status = 200)` | JSON text you already serialized |
| `bytes(data, status = 200)` | Binary data |
| `empty(status = 204)` | No body |
| `detail(message, status = 500)` | `{"detail": message}` as JSON |
| `from_status(status)` | The code and reason of an `HttpStatus` |
| `not_found()` | 404 |

Chain `set_header(name, value)` to add a header; it returns the same response.

| `HttpStatus` factory | Code |
| --- | --- |
| `bad_request()` | 400 |
| `unauthorized()` | 401 |
| `forbidden()` | 403 |
| `not_found()` | 404 |
| `conflict()` | 409 |
| `unprocessable()` | 422 |
| `too_many()` | 429 |
| `internal()` | 500 |

For any other code, construct one directly: `HttpStatus(402, "Payment Required")`.

## Status codes the framework sends

| Status | When |
| --- | --- |
| 400 | The request body cannot be read, or a multipart form exceeds its [limits](uploads.md#form-limits). |
| 401 | Authentication is missing or invalid. |
| 403 | The user lacks a required scope, or a WebSocket handler closed before accepting. |
| 404 | No route matches the path. |
| 405 | The path matches a route, but not for this method. The `Allow` header lists the methods it accepts. |
| 431 | The request headers exceed `ServerConfig.max_header_bytes`. |
| 503 | `ServerConfig.limit_concurrency` is exceeded, or the server is shutting down. |
| 422 | Input validation failed. The body is a `detail` array whose items have `loc`, `type`, and `msg`. |

A middleware layer may return a response without invoking the endpoint.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
