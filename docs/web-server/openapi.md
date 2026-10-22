---
title: Swagger and OpenAPI
summary: Publish interactive API documentation.
icon: material/api
---

# Swagger and OpenAPI

Configure documentation assets and behavior with typed options. Host the JS/CSS assets yourself by changing their URLs.

```dream
import system;
import system.webapi;

@router("/items")
class Routes {
    @operation("Get item", "Returns one item", "items", 200)
    @get("/{id}")
    public static fun item(@path id: int): string {
        return id.to_string();
    }
}

async fun main(): void {
    let options = WebAppOptions(title: "Inventory", version: "2.0.0", docs_url: "/api-docs");
    options.swagger = SwaggerOptions(
        js_url: "/assets/swagger.js",
        css_url: "/assets/swagger.css",
        deep_linking: true,
        persist_authorization: false
    );
    let app = WebApp(options);
    app.include_router(Routes.router()).unwrap();
    System.println(app.openapi());
}
```

The example prints the document without starting a listener. If you serve `/api-docs`, the asset URLs must be served by your deployment or by explicit routes.

OpenAPI 3.0.3 contains mounted paths, parameter sources, shared model schemas, request bodies, content types, operation metadata, and validation responses. Mounted prefixes are reflected in paths. Unmounted routers are absent. WebSocket endpoints are not represented as ordinary OpenAPI operations.

## Operation attributes

Arguments are positional, and trailing optional arguments can be left off.

| Attribute | Arguments, in order | Effect |
| --- | --- | --- |
| `@operation(...)` | `summary`, `description`, `tags`, `status`, `deprecated` | Describes the route. `tags` is a comma-separated string. `status` also replaces the 200 (or 204 for `void`) the handler would otherwise send. All arguments are optional. |
| `@response(...)` | `status`, `description`, `model`, `content_type` | Documents one more response, such as a 404, without changing behavior. `model` names an `@json` type for the body. Repeatable. |
| `@security_scheme(...)` | `id`, `kind`, `scheme`, `name`, `location` | On a dependency provider: declares how clients authenticate. `kind` is `http`, `apiKey`, or `oauth2`; `location` is `header`, `query`, or `cookie`. |
| `@oauth2_scheme(...)` | `id`, `authorization_url`, `token_url`, `scopes` | On a dependency provider: declares an OAuth2 authorization-code flow for Swagger's Authorize button. |
| `@requires_scopes(scopes)` | Space-separated scope names | Lists required scopes on the operation; see [authorization](authorization.md). |

Without `@operation`, a route documents status 200, or 204 when the handler returns `void`. Every operation also lists a 422 response for input validation failures.

## Built-in security providers

These providers in `system.webapi` already carry `@security_scheme`, so injecting one with `@dep` adds the matching Swagger Authorize control.

| Provider | Reads | Returns | Scheme |
| --- | --- | --- | --- |
| `HttpAuth.bearer_token` | `Authorization: Bearer <token>` | The token | `BearerAuth` (`http`, `bearer`) |
| `HttpAuth.basic_auth` | `Authorization: Basic <value>` | The encoded value after `Basic ` | `BasicAuth` (`http`, `basic`) |
| `HttpAuth.api_key_header` | `X-Api-Key` header | The key | `ApiKeyAuth` (`apiKey` in a header) |

Each returns 401 when the header is missing or malformed. They only extract credentials; your own dependency must check them. Metadata describes a check; include the actual provider or authentication middleware to enforce it.

## Swagger options

Set these on `options.swagger`, a `SwaggerOptions` value.

| Field | Default | Effect |
| --- | --- | --- |
| `js_url` | unpkg `swagger-ui-dist@5` bundle | Swagger UI script. Point it at your own copy to self-host. |
| `css_url` | unpkg `swagger-ui-dist@5` stylesheet | Swagger UI stylesheet. |
| `redoc_js_url` | jsDelivr `redoc@2` bundle | ReDoc script. |
| `deep_linking` | `true` | Give each operation its own URL fragment. |
| `persist_authorization` | `false` | Keep entered credentials across page reloads. |
| `try_it_out_enabled` | `false` | Open operations with "Try it out" already enabled. |
| `oauth_client_id` | `""` | Client ID Swagger uses for OAuth login. |
| `oauth_use_pkce` | `true` | Use PKCE in Swagger's OAuth login. |
| `oauth_redirect_url` | `""` | Redirect URL after Swagger's OAuth login. |

Client secrets belong on the server, never in Swagger UI configuration. Leave OpenAPI enabled when serving Swagger or ReDoc; disabling its endpoint leaves those viewers without a document to load. The documentation URLs themselves are set on [`WebAppOptions`](configuration.md#app-options-webappoptions).

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
