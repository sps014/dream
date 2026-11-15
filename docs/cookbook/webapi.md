---
title: Native JSON API
summary: Serve a JSON greeting from a URL parameter.
icon: material/api
---

# Serve a JSON API

Save this as `app.dream`. It accepts a name from the URL and returns a JSON greeting.

```dream
import system;
import system.json;
import system.webapi;

@json
class Greeting {
    public message: string;

    public constructor(message: string) {
        this.message = message;
    }
}

@router("")
class Routes {
    @get("/hello/{name}")
    public static fun hello(@path name: string): Greeting {
        return Greeting("Hello, " + name);
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

Run `dream run app.dream`, then open `http://127.0.0.1:8000/hello/Dream` for `{"message":"Hello, Dream"}` with content type `application/json`. Open `/docs` to try the endpoint, and stop the server with Ctrl+C.

`@path` reads the URL parameter, and `@json` lets the handler return a typed object. Configuration and startup return `Result`; this example uses `unwrap`, while an app that needs recovery should handle those errors before it accepts requests.

Continue with [validated models](../web-server/validation.md),
[dependencies](../web-server/dependencies.md), or the
[complete service example](../web-server/example.md).
