---
title: Configuration
summary: "Configure an app with a `WebAppOptions` snapshot."
icon: material/cog-outline
---

# Application configuration

Pass a `WebAppOptions` snapshot to the app constructor. Changes to the original options afterward do not change the app.

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
    let port = app.listen("127.0.0.1", 0).await.unwrap();
    System.println("Listening on " + port.to_string());
    app.shutdown();
    app.wait().await;
}
```

## Options

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
| `tls_cert_path` | `""` | Server certificate file. Leave empty to serve plain HTTP. |
| `tls_key_path` | `""` | Private key file for the certificate. |
| `swagger` | `SwaggerOptions()` | Swagger UI assets and behavior; see [Swagger options](openapi.md#swagger-options). |

## Running the app

| `WebApp` method | Effect |
| --- | --- |
| `listen(host, port, token?)` | Runs startup hooks, starts accepting connections, and returns the bound port. Port `0` picks a free port. |
| `run(host, port, token?)` | Listens, then waits until the server stops. |
| `shutdown()` | Stops accepting connections and cancels pending transport work. |
| `wait(token?)` | Waits for active requests, their cleanup, and shutdown hooks to finish. |
| `openapi()` | Returns the OpenAPI document as JSON text, without listening. |

Call `shutdown` followed by `wait` to release the listener and finish cleanup.

Mount routers, install middleware and overrides, and register lifespan hooks before startup. Startup freezes configuration; later configuration calls return an error. Duplicate routes and documentation URL collisions fail before accepting traffic. Treat each app as one server lifetime: construct another instance to run a fresh server.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
