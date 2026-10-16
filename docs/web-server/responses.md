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

Strings become text, byte arrays become binary responses, and `@json` models become JSON. `HttpOutgoing` factories include `text`, `html`, `json_text`, `bytes`, `empty`, `detail`, and `from_status`.

`Result<T, HttpStatus>` returns the supplied failure status. `Result<T, string>` produces a 500 JSON detail response. Prefer intentional client-safe errors; do not put database credentials or internal exceptions into returned strings.

Validation failures use HTTP 422 with a `detail` array. Each item has `loc`, `type`, and `msg`. Authentication is 401, authorization is 403, and an unmatched route is 404. A middleware layer may return a response without invoking the endpoint.

Use operation metadata to describe a success response: `@operation("Create item", "Creates one item", "items", 201)` sets summary, description, comma-separated tags, and success status. Return an explicit `HttpOutgoing` for conditional status behavior.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
