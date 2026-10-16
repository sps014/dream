# Swagger and OpenAPI

Configure documentation assets and behavior with typed options. Host the JS/CSS assets yourself by changing their URLs.

```dream
import system;
import system.webapi;

@router("/items")
class Routes {
    @operation("Get item", "Returns one item", "items", 200)
    @get("/{id}")
    public static fun item(@path id: int): string { return id.to_string(); }
}
async fun main(): void {
    let options = WebAppOptions(title: "Inventory", version: "2.0.0", docs_url: "/api-docs");
    options.swagger = SwaggerOptions(js_url: "/assets/swagger.js", css_url: "/assets/swagger.css", deep_linking: true, persist_authorization: false);
    let app = WebApp(options); app.include_router(Routes.router()).unwrap();
    System.println(app.openapi());
}
```

The example prints the document without starting a listener. If you serve `/api-docs`, the asset URLs must be served by your deployment or by explicit routes. Default assets load from the configured CDNs. `persist_authorization` defaults to false, `deep_linking` to true, and `try_it_out_enabled` to false.

OpenAPI 3.0.3 contains mounted paths, parameter sources, shared model schemas, request bodies, content types, operation metadata, and validation responses. Mounted prefixes are reflected in paths. Unmounted routers are absent. WebSocket endpoints are not represented as ordinary OpenAPI operations.

Security dependencies declare schemes with `@security_scheme`. Builtin Bearer, Basic, and API-key helpers supply metadata, and protected endpoints expose Swagger Authorize controls. Metadata describes a check; include the actual provider or authentication middleware to enforce it.

For OAuth-capable Swagger clients, configure `oauth_client_id`, `oauth_use_pkce`, and `oauth_redirect_url`. Client secrets belong on the server, never in Swagger UI configuration. Leave OpenAPI enabled when serving Swagger or ReDoc; disabling its endpoint leaves those viewers without a document to load.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
