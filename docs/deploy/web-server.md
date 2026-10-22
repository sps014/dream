---
title: TLS and web servers
summary: Serve HTTPS directly or behind a TLS proxy, and size the server for production.
icon: material/lock-check-outline
---

# TLS and deployment

Set both PEM paths in `ServerConfig` to terminate TLS in the native listener, or bind privately behind your existing TLS proxy.

```dream
import system;
import system.webapi;

@router("")
class Routes {
    @get("/health")
    public static fun health(): string {
        return "ok";
    }
}

async fun main(): void {
    let app = WebApp(WebAppOptions(title: "Production API"));
    app.include_router(Routes.router()).unwrap();

    let config = ServerConfig(
        ssl_certfile: System.env("TLS_CERT").unwrap(),
        ssl_keyfile: System.env("TLS_KEY").unwrap(),
        limit_concurrency: 2000,
        timeout_graceful_shutdown_ms: 30000
    );
    app.run("0.0.0.0", 8443, config).await.unwrap();
}
```

These settings matter most when deploying. [Configuration](../web-server/configuration.md#server-settings-serverconfig) lists every `ServerConfig` field.

| Setting | Default | Meaning |
|---|---|---|
| `ServerConfig.ssl_certfile` | `""` | Server certificate (PEM); empty serves plain HTTP. |
| `ServerConfig.ssl_keyfile` | `""` | Private key (PEM) for the certificate. |
| `ServerConfig.limit_concurrency` | `0` | Above this many open connections or in-flight requests, new requests get `503`. `0` means no limit. |
| `ServerConfig.timeout_keep_alive_ms` | `5000` | Idle keep-alive connections, and clients that send headers too slowly, are closed after this long. |
| `ServerConfig.timeout_graceful_shutdown_ms` | `0` | How long shutdown waits for running requests before closing them. `0` waits for all of them. |
| `WebAppOptions.docs_url` | `"/docs"` | Swagger UI path; empty disables it. |
| `WebAppOptions.openapi_url` | `"/openapi.json"` | OpenAPI document path; empty disables it. |
| `WebAppOptions.redoc_url` | `"/redoc"` | ReDoc path; empty disables it. |

Supplying only one TLS path, invalid credentials, or a bind failure returns `IoError`. Certificate/key files are deployment inputs. Run the service with access to the required native capability libraries, or use Dream's relocatable build packaging.

## Request size limits

Like FastAPI and Uvicorn, the server does not cap the size of request bodies. Routes that read the whole body (`@body`, `read_body_bytes`, `read_body_text`) hold it in memory, so set a limit in front of the service. With nginx, for example:

```nginx
client_max_body_size 20m;
```

Routes that accept large inputs should read them with `incoming.stream()` or `form()`, which keep memory bounded; see [Uploads](../web-server/uploads.md). Request headers are limited by `ServerConfig.max_header_bytes`.

## Behind a reverse proxy

Bind to a private address, configure TLS and request limits at the proxy, and register externally visible OIDC callback URLs. The service does not infer trust from arbitrary forwarded headers. Browser authentication cookies require HTTPS.

Set `limit_concurrency` to the load one instance can serve. Requests above it fail fast with `503`, which a proxy or load balancer can retry on another instance instead of queueing them.

Decide whether interactive docs should be enabled in each environment. Disable docs, OpenAPI, and ReDoc independently with empty URLs; self-host assets when your deployment must avoid CDN requests. Ensure the configured docs and authentication endpoints do not collide with mounted routes.

## Stopping the service

On termination, call `shutdown`, then await `wait`. The server stops accepting connections, lets running requests finish, sends WebSocket clients a `1012` close frame, and then runs lifespan shutdown hooks. Set `timeout_graceful_shutdown_ms` below your supervisor's kill timeout so slow requests are cut off before the process is killed.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](../web-server/example.md).
