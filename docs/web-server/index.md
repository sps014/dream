---
title: Web server
summary: "Typed HTTP services with routers, validation, auth, OpenAPI, and WebSockets."
icon: material/server-network
hide: [navigation, toc]
---

# Web APIs

Build native HTTP services with typed handlers, explicit application instances, and generated OpenAPI documentation. Dream keeps routing and dependency resolution in compile-time generators; application configuration belongs to ordinary Dream objects.

Start with [your first application](first-application.md), then learn [configuration](configuration.md) and [routers](routers.md). Continue with [parameters and bodies](parameters.md), [model validation](validation.md), and [responses](responses.md).

For application structure, read [dependencies](dependencies.md), [resource cleanup and overrides](resources.md), [lifecycle and state](lifecycle.md), and [middleware and CORS](middleware.md).

Publish interactive documentation with [Swagger and OpenAPI](openapi.md). Protect routes with [local authentication](authentication.md), [external OAuth2/OIDC login](oidc.md), and [authorization scopes](authorization.md).

Transport guides cover [uploads](uploads.md), [streaming and SSE](streaming.md), and [WebSockets](websockets.md). Finish with [testing](testing.md), [TLS and deployment](../deploy/web-server.md), and the [multifile service example](example.md).

## Guides in this section

<!-- cards -->

## Requirements

HTTP servers require a native Dream target and the `dream-host-net` capability. Local authentication additionally uses `dream-host-crypto`. Server calls in WebAssembly programs receive compile-time availability diagnostics. The HTTP client remains available separately through `system.net`.

Applications mount routers explicitly. Importing a file does not expose its handlers. Two apps may mount the same router with different prefixes and have independent middleware, state, documentation, dependency overrides, and shutdown.
