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

`WebApp.listen` binds (port `0` is ephemeral) and starts the accept loop in the background, returning the bound port. `WebApp.run` is listen + wait until `shutdown`. Pass `token: Some(tok)` to `listen` / `run` / `wait` to shut the listener down when the token is cancelled (`RequestContext.cancellation_token` exposes the same token to handlers). OpenAPI (`/openapi.json`), Swagger UI (`/docs`), and ReDoc (`/redoc`) are on by default; set `docs_url` / `openapi_url` / `redoc_url` to `""` on `WebAppOptions` to disable.

Optional in-process TLS: set both `tls_cert_path` and `tls_key_path` on `WebAppOptions` (PEM files read by the host). Leave them empty to stay on cleartext HTTP, or terminate TLS at a proxy.

## Routes

`@get` / `@post` / `@put` / `@patch` / `@delete` / `@head` / `@options` take a path. `{name}` binds a parameter of the same name (or `@path("name")`).

`@http_group("/api")` on a **class** prefixes every route method. Methods must be `public static`. Shared `@use` on the class applies to every method.

| Extractor | Meaning |
| --- | --- |
| path segment / `@path` | `{id}` → `id: int` or `string` |
| `@query` / `@query("q")` | query string; `Option<string>` if optional |
| `@header("Authorization")` | request header |
| `@cookie("sid")` | cookie |
| `@body` | JSON (`@json` type), `string`, or `byte[]` |
| `@form` / `@file` | `multipart/form-data` field or `UploadedFile` (not with `@body`) |
| `@dep(fn)` | call `fn` (FastAPI `Depends`); `Result<T, HttpStatus>` short-circuits |

Handlers may return a `@json` type, `string`, `HttpOutgoing`, `EventStream`, or `Result<T, HttpStatus>`. Other `Result<T, E>` errors become `500` JSON `{"detail":"..."}` (FastAPI-style). Language panics still abort the process.

## Middleware

`@middleware async fun name(ctx: RequestContext, next: Next): HttpOutgoing` wraps every route (including `/docs` and `/openapi.json`). `@use(name)` on a route (repeatable) or `@http_group` class adds layers **inside** app-wide middleware (later `@use` is closer to the handler). `WebApp.use(Middleware(fn))` registers at runtime. `RequestContext.set` / `get` is a string bag for request-scoped data.

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

## Streaming and WebSocket

Return `EventStream` from a `@get` handler and `await stream.send(event, data)` for SSE (`text/event-stream`). A cancelled token (argument or the server token) makes `send` return without writing.

`@websocket("/ws") async fun echo(ws: ServerWebSocket): void` upgrades after middleware (CORS/auth can still reject the handshake). Frame types are `system.net` `WebSocketMessage`; the client is `WebSocket.connect("ws://...")`. `send_text` / `send_binary` / `receive` take an optional token and return `NetError.cancelled()` when it has fired.

Pair with [JSON](json.md) and the [HTTP client](http.md). Sample: `sample/webapi/app.dream`.
