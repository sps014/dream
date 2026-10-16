# system.webapi: API lookup

Read the [Web APIs guide](../../web-server/index.md) for complete examples and behavior. Applications own their routes, options, middleware, lifecycle, and dependency overrides.

- [Application](webapi-web-app.md)
- [Application options](webapi-web-app-options.md)
- [Swagger options](webapi-swagger-options.md)
- [Routers and endpoints](webapi-web-router.md)
- [Request context and middleware](webapi-middleware.md)
- [Dependency resources and overrides](webapi-dependency-resource.md)
- [Attributes](webapi-webapi-attributes.md)
- [Incoming requests](webapi-http-incoming.md)
- [Outgoing responses](webapi-http-outgoing.md)
- [HTTP statuses](webapi-http-status.md)
- [CORS](webapi-cors.md)
- [Header authentication helpers](webapi-auth.md)
- [Server WebSockets](webapi-server-ws.md)
- [SSE events](webapi-sse-event.md)
- [Uploaded files](webapi-uploaded-file.md)
- [Standalone validation](system-validation.md)
- [Authentication services](system-webapi-auth.md)

`@router` generates a static `router(): WebRouter` method on its class. `@dependency_override` generates a static `dependencies(): DependencyOverrides` method on the containing replacement class. Install their results explicitly on an app before startup. Server entry points are instance methods and native only.
