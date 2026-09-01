# system.webapi

Native HTTP server (`dream run` / libdream). Browser and wasm32 cannot bind a TCP listener; calling `WebApp.listen` there is a compile error.

**Import:** `import system.webapi;`

```dream
import system;
import system.json;
import system.webapi;

@get("/health")
fun health(): string {
    return "ok";
}

async fun main(): void {
    await WebApp.run("127.0.0.1", 8080);
}
```

`WebApp.listen` binds (port `0` is ephemeral) and starts the accept loop in the background, returning the bound port. `WebApp.run` is listen + wait until `shutdown`. OpenAPI (`/openapi.json`) and Swagger UI (`/docs`) are on by default; set `docs_url` / `openapi_url` to `""` on `WebAppOptions` to disable.

## Routes

`@get` / `@post` / `@put` / `@patch` / `@delete` / `@head` / `@options` take a path. `{name}` binds a parameter of the same name (or `@path("name")`).

| Extractor | Meaning |
| --- | --- |
| path segment / `@path` | `{id}` → `id: int` or `string` |
| `@query` / `@query("q")` | query string; `Option<string>` if optional |
| `@header("Authorization")` | request header |
| `@cookie("sid")` | cookie |
| `@body` | JSON (`@json` type), `string`, or `byte[]` |
| `@dep(fn)` | call `fn` (FastAPI `Depends`); `Result<T, HttpStatus>` short-circuits |

Handlers may return a `@json` type, `string`, `HttpOutgoing`, or `Result<T, HttpStatus>`.

## Middleware

`@middleware async fun name(ctx: RequestContext, next: Next): HttpOutgoing` wraps every route (including `/docs` and `/openapi.json`). `WebApp.use(Middleware(fn))` registers at runtime. `RequestContext.set` / `get` is a string bag for request-scoped data.

CORS matches FastAPI `CORSMiddleware`:

```dream
WebApp.use(CORS(CorsOptions(
    allow_origins: "*",
    allow_methods: "*",
    allow_headers: "*",
)));
```

`CorsOptions` fields: `allow_origins`, `allow_methods`, `allow_headers` (comma-separated or `"*"`), `expose_headers`, `allow_credentials`, `max_age` (seconds, default `600`). Preflight `OPTIONS` with `Access-Control-Request-Method` is answered with `200` and does not run the route.

Stdlib auth helpers: `BearerToken`, `ApiKeyHeader`, `BasicAuth` — pass them to `@dep(...)`.

Pair with [JSON](json.md) and the [HTTP client](http.md). Sample: `sample/webapi/app.dream`.
