---
title: Parameters and bodies
summary: "Bind path, query, header, and body inputs to handler parameters."
icon: material/import
---

# Parameters and request bodies

Use parameter attributes to identify input sources. Required inputs must be present; `Option<T>` accepts absence.

```dream
import system;
import system.webapi;
import system.json;

@json
class Message {
    public text: string;

    public constructor(text: string) {
        this.text = text;
    }
}

@router("")
class Routes {
    @get("/items/{id}")
    public static fun item(
        @path id: int,
        @query limit: Option<int>,
        @header("X-Request-ID") request_id: Option<string>
    ): string {
        return id.to_string()
            + ":" + limit.unwrap_or(10)
            .to_string() + ":" + request_id.unwrap_or("");
    }

    @post("/messages")
    public static fun message(@body body: Message): Message {
        return body;
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

## Input attributes

| Attribute | Reads from | Accepted parameter types | Name override |
| --- | --- | --- | --- |
| `@path` | A `{name}` segment of the route path | Scalars: `string`, `bool`, numbers | `@path("id")` |
| `@query` | The first query-string value with that name | Scalars, or `Option<T>` of a scalar | `@query("page_size")` |
| `@header` | The first request header with that name | Scalars, or `Option<T>` of a scalar | `@header("X-Request-ID")` |
| `@cookie` | A cookie with that name, not URL-decoded | Scalars, or `Option<T>` of a scalar | `@cookie("session")` |
| `@body` | The whole request body | An `@json` model, `JsonValue`, `string`, or `byte[]` | — |
| `@form` | A field of a `multipart/form-data` body | Scalars, or `Option<T>` of a scalar | `@form("title")` |
| `@file` | A file part of a `multipart/form-data` body | `UploadedFile` or `Option<UploadedFile>` | `@file("attachment")` |
| `@dep(provider)` | The value returned by a provider function | The provider's value type | — |

Without an override, the parameter name is the input name. A parameter matching a `{path}` segment can bind without an explicit `@path`. A parameter that has no matching segment and no input attribute is a compile-time error. A handler can also take `RequestContext` to read the raw request, or `ServerWebSocket` on a `@websocket` route.

Scalar values are parsed with the declared type's `parse`, so `@query limit: int` rejects `?limit=ten`.

For `@body`, a model uses strict [validation](validation.md). `JsonValue` parses any JSON without a model. `string` and `byte[]` give you the raw body for your own processing. A route cannot combine `@body` with `@form` or `@file`.

## Defaults and constraints

Use `@default_json("10")` on a scalar parameter to supply a value when it is absent. The constraint attributes `@minimum`, `@maximum`, `@min_length`, `@max_length`, and `@pattern` also work on scalar parameters; the [validation guide](validation.md#constraint-attributes) lists what each one checks.

Attribute arguments are positional. Ordinary function and constructor calls support named arguments.

## Input errors

| Situation | Status | Error `type` |
| --- | --- | --- |
| A required parameter is missing | 422 | `missing` |
| A scalar value cannot be parsed as its type | 422 | `type` |
| A value fails a constraint attribute | 422 | `minimum`, `max_length`, and so on |
| A `@body` model fails validation | 422 | The [validation code](validation.md#error-codes) |
| A body is not valid JSON | 422 | `json` |
| A required header is missing inside a dependency provider | 401 | — |
| The request body cannot be read | 400 | — |
| The request body exceeds the 16 MiB host limit | 413 | — |

The 422 responses use the same `{"detail": [{"loc", "type", "msg"}]}` shape as model validation.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
