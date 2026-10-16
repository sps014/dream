---
title: TLS and web servers
summary: Serve HTTPS directly or behind a TLS proxy.
icon: material/lock-check-outline
---

# TLS and deployment

Set both PEM paths to terminate TLS in the native listener, or bind privately behind your existing TLS proxy.

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
    let options = WebAppOptions(
        title: "Production API",
        tls_cert_path: System.env("TLS_CERT").unwrap(),
        tls_key_path: System.env("TLS_KEY").unwrap()
    );
    let app = WebApp(options);
    app.include_router(Routes.router()).unwrap();
    app.run("0.0.0.0", 8443).await.unwrap();
}
```

These `WebAppOptions` settings matter when deploying:

| Option | Default | Meaning |
|---|---|---|
| `tls_cert_path` | `""` | Server certificate (PEM); empty serves plain HTTP |
| `tls_key_path` | `""` | Private key (PEM) for the certificate |
| `docs_url` | `"/docs"` | Swagger UI path; empty disables it |
| `openapi_url` | `"/openapi.json"` | OpenAPI document path; empty disables it |
| `redoc_url` | `"/redoc"` | ReDoc path; empty disables it |

Supplying only one TLS path, invalid credentials, or a bind failure returns `IoError`. Certificate/key files are deployment inputs. Run the service with access to the required native capability libraries, or use Dream's relocatable build packaging.

For a reverse proxy, bind to a private address, configure TLS and request limits at the proxy, and register externally visible OIDC callback URLs. The service does not infer trust from arbitrary forwarded headers. Browser authentication cookies require HTTPS.

Decide whether interactive docs should be enabled in each environment. Disable docs, OpenAPI, and ReDoc independently with empty URLs; self-host assets when your deployment must avoid CDN requests. Ensure the configured docs and authentication endpoints do not collide with mounted routes.

On termination, stop acceptance with `shutdown`, then await `wait`. This lets request cleanup complete before application lifespan shutdown. Include appropriate process termination handling in your deployment supervisor.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](../web-server/example.md).
