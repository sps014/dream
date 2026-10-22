---
title: Configuration
summary: "Describe the API with `WebAppOptions` and tune the server with `ServerConfig`."
icon: material/cog-outline
---

# Application configuration

Settings come in two groups, the same split FastAPI and Uvicorn make:

- `WebAppOptions` describes the application: its title, version, and documentation pages. Pass it to the `WebApp` constructor.
- `ServerConfig` controls the network server: TLS, connection limits, timeouts, and WebSocket limits. Pass it to `listen` or `run`.

```dream
import system;
import system.webapi;

async fun main(): void {
    let options = WebAppOptions(
        title: "Orders API",
        version: "1.0.0",
        docs_url: "/documentation",
        redoc_url: "",
        description: "Manage orders"
    );
    options.swagger = SwaggerOptions(persist_authorization: false, deep_linking: true);
    let app = WebApp(options);

    let config = ServerConfig(limit_concurrency: 1000, timeout_graceful_shutdown_ms: 30000);
    let port = app.listen("127.0.0.1", 0, config).await.unwrap();
    System.println("Listening on " + port.to_string());

    app.shutdown();
    app.wait().await;
}
```

The app takes a snapshot of `WebAppOptions`, so later changes to the original object do not affect it.

## App options (`WebAppOptions`)

Every `WebAppOptions` constructor argument has a default, so pass only what you change.

| Field | Default | Effect |
| --- | --- | --- |
| `title` | `"Dream API"` | API title in the OpenAPI document and documentation pages. |
| `version` | `"0.1.0"` | API version in the OpenAPI document. |
| `summary` | `""` | Short API summary in the OpenAPI document. |
| `description` | `""` | Longer API description in the OpenAPI document. |
| `terms_of_service` | `""` | Terms-of-service URL in the OpenAPI document. |
| `docs_url` | `"/docs"` | Path of the Swagger UI page. `""` disables it. |
| `openapi_url` | `"/openapi.json"` | Path of the OpenAPI document. |
| `redoc_url` | `"/redoc"` | Path of the ReDoc page. `""` disables it. |
| `swagger` | `SwaggerOptions()` | Swagger UI assets and behavior; see [Swagger options](openapi.md#swagger-options). |

## Server settings (`ServerConfig`)

The field names match Uvicorn's options, and every default matches Uvicorn's. Durations are in milliseconds. A limit or timeout of `0` means "none". A negative value makes `listen` fail with `EINVAL`.

| Field | Default | Effect |
| --- | --- | --- |
| `ssl_certfile` | `""` | PEM certificate file. Leave it and `ssl_keyfile` empty to serve plain HTTP. |
| `ssl_keyfile` | `""` | PEM private key for `ssl_certfile`. Setting only one of the two fails with `EINVAL`. |
| `limit_concurrency` | `0` | When more connections are open, or more requests are in flight, than this number, new requests get `503 Service Unavailable`. `0` accepts everything. |
| `backlog` | `2048` | How many incoming connections the operating system queues before the server accepts them. |
| `timeout_keep_alive_ms` | `5000` | How long an idle keep-alive connection, or a client still sending its request headers, may stay silent before the server closes it. |
| `timeout_graceful_shutdown_ms` | `0` | How long `shutdown` waits for in-flight requests before closing their connections. `0` waits until every request finishes. |
| `max_header_bytes` | `16384` | Largest request line plus headers, in bytes. Larger requests get `431 Request Header Fields Too Large`. Must be at least `8192`. |
| `ws_max_size` | `16777216` | Largest WebSocket message, in bytes (16 MiB). A larger message closes the socket with code `1009`. `0` removes the limit. |
| `ws_max_queue` | `32` | Inbound WebSocket messages buffered per socket. When the queue is full, the server stops reading from that client until the handler catches up. |
| `ws_ping_interval_ms` | `20000` | How often the server pings each WebSocket client. `0` disables pings. |
| `ws_ping_timeout_ms` | `20000` | How long a client has to answer a ping. A late answer closes the socket with code `1011`. |
| `proxy_headers` | `true` | Read the client address and scheme from `X-Forwarded-For` and `X-Forwarded-Proto` when the connecting peer is a trusted proxy. |
| `forwarded_allow_ips` | `"127.0.0.1"` | Trusted proxies: a comma-separated list of addresses and CIDR networks such as `"127.0.0.1,10.0.0.0/8"`. `"*"` trusts every peer. An invalid entry makes `listen` fail with `EINVAL`. |
| `h2_max_concurrent_streams` | `100` | Most requests one HTTP/2 connection may have in progress at once. Must be at least `1`. |

The server has no limit on request body size, and neither do FastAPI and Uvicorn. Put a limit at your reverse proxy (see [Deploying a web server](../deploy/web-server.md)), or read large bodies with [`stream()`](streaming.md#reading-a-request-body-as-a-stream) and [`form()`](uploads.md).

### Serving HTTPS

```dream
import system;
import system.webapi;

async fun main(): void {
    let app = WebApp();
    let config = ServerConfig(ssl_certfile: "certs/server.pem", ssl_keyfile: "certs/server.key");
    let result = app.run("0.0.0.0", 8443, config).await;
    if result.is_err() {
        System.println("cannot start: " + result.unwrap_err().message());
    }
}
```

A missing or unreadable file makes `listen` and `run` fail before the server accepts traffic.

### HTTP/2

The server speaks HTTP/1.1 and HTTP/2 on the same port, with no setting to turn on. Over HTTPS, a client that offers HTTP/2 during the TLS handshake gets it. Over plain HTTP, a client that opens the connection with the HTTP/2 preface ("prior knowledge", such as `curl --http2-prior-knowledge`) gets HTTP/2. Everyone else gets HTTP/1.1.

Handlers see no difference between the two. Each HTTP/2 request counts against `limit_concurrency` like an HTTP/1.1 request, and `h2_max_concurrent_streams` caps how many run at once on one connection. WebSockets always use HTTP/1.1, so a WebSocket client connects over HTTP/1.1 even when the server supports HTTP/2.

### Behind a reverse proxy

A reverse proxy such as nginx connects to your server itself, so every request appears to come from the proxy's address. Proxies report the real client in the `X-Forwarded-For` header and the original scheme in `X-Forwarded-Proto`. Each request exposes the result on `HttpIncoming`:

| Field | Value |
| --- | --- |
| `client_host` | The connecting peer's address, or the client named in `X-Forwarded-For` when the peer is trusted. |
| `client_port` | The peer's port, or `0` when `client_host` came from `X-Forwarded-For`. |
| `scheme` | `"https"` for a TLS connection, otherwise `"http"`, or the trusted proxy's `X-Forwarded-Proto`. |

The server believes these headers only from peers listed in `forwarded_allow_ips`; from anyone else they are ignored, so a client cannot fake its address. When `X-Forwarded-For` lists several hops, the server reads from the right and takes the first address that is not a trusted proxy. Set `forwarded_allow_ips` to your proxy's address:

```dream
import system;
import system.webapi;

async fun main(): void {
    let app = WebApp();
    let config = ServerConfig(forwarded_allow_ips: "10.0.0.0/8");
    let result = app.run("0.0.0.0", 8000, config).await;
    if result.is_err() {
        System.println("cannot start: " + result.unwrap_err().message());
    }
}
```

Set `proxy_headers: false` when no proxy sits in front of the server and you want the raw peer address on every request.

## Running the app

| `WebApp` method | Effect |
| --- | --- |
| `listen(host, port, token?)` | Runs startup hooks, starts accepting connections with the default `ServerConfig`, and returns the bound port. Port `0` picks a free port. |
| `listen(host, port, config, token?)` | The same, with your `ServerConfig`. |
| `run(host, port, token?)` / `run(host, port, config, token?)` | Listens, then waits until the server stops. |
| `shutdown()` | Stops accepting connections and starts a graceful shutdown. |
| `wait(token?)` | Waits for active requests, their cleanup, and shutdown hooks to finish. |
| `openapi()` | Returns the OpenAPI document as JSON text, without listening. |

Call `shutdown` followed by `wait` to release the listener and finish cleanup. A graceful shutdown works like Uvicorn's:

1. The server stops accepting connections, and closes keep-alive connections that have no request in progress.
2. Requests already running finish normally. Requests that arrive during shutdown get `503`.
3. Server WebSockets receive a close frame with code `1012` (service restart).
4. When `timeout_graceful_shutdown_ms` is set and requests are still running after it, their connections close and their handlers are cancelled.
5. Shutdown hooks registered with `lifespan` run last.

Mount routers, install middleware and overrides, and register lifespan hooks before startup. Startup freezes configuration; later configuration calls return an error. Duplicate routes and documentation URL collisions fail before accepting traffic. Treat each app as one server lifetime: construct another instance to run a fresh server.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
