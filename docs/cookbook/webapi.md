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

Run `dream run app.dream`, then open `http://127.0.0.1:8000/hello/Dream`.
The response is `{"message":"Hello, Dream"}` with the `application/json` content type.
Open `/docs` to try the endpoint interactively. Stop the server with Ctrl+C.

`@path` reads the URL parameter. `@json` lets the handler return a typed object.
Configuration and startup return `Result`; this small example uses `unwrap` to report
failures immediately. An application that needs recovery should inspect those results
and handle the errors before accepting requests.

Continue with [validated models](../web-apis/validation.md),
[dependencies](../web-apis/dependencies.md), or the
[complete service example](../web-apis/example.md).
