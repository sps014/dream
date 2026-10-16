# Parameters and request bodies

Use parameter attributes to identify input sources. Required inputs must be present; `Option<T>` accepts absence.

```dream
import system;
import system.webapi;
import system.json;

@json
class Message {
    public text: string;
    public constructor(text: string) { this.text = text; }
}
@router("")
class Routes {
    @get("/items/{id}")
    public static fun item(@path id: int, @query limit: Option<int>, @header("X-Request-ID") request_id: Option<string>): string {
        return id.to_string() + ":" + limit.unwrap_or(10).to_string() + ":" + request_id.unwrap_or("");
    }
    @post("/messages")
    public static fun message(@body body: Message): Message { return body; }
}
async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

`@path`, `@query`, `@header`, and `@cookie` accept a positional name override, such as `@query("page_size")`. A parameter matching a `{path}` segment can bind without an explicit `@path`. A parameter without a matching segment or an extractor produces a compile-time diagnostic.

`@body` accepts an `@json` model, `JsonValue`, `string`, or `byte[]`. Models use strict validation. `JsonValue` parses JSON without a declared model. Text and bytes preserve the body for custom processing.

Use `@default_json("10")` on a scalar parameter to supply an absent value. Validation attributes such as `@minimum(1.0)` and `@max_length(100)` apply to extracted values. Missing or malformed values return HTTP 422 with structured locations and error codes. Missing authentication headers in provider functions return 401. Body transport failures return 400; host body limits remain 16 MiB and return 413 when exceeded.

Attribute arguments are positional. Ordinary function and constructor calls support named arguments.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
