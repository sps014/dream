---
title: Web server
summary: "Typed HTTP services with routers, validation, auth, OpenAPI, and WebSockets."
icon: material/server-network
hide: [toc]
---

# Web APIs

Build native HTTP services with typed handlers, explicit application instances, and generated OpenAPI documentation. Dream keeps routing and dependency resolution in compile-time generators; application configuration belongs to ordinary Dream objects.

Start with [your first application](first-application.md), then learn [configuration](configuration.md) and [routers](routers.md). Continue with [parameters and bodies](parameters.md), [model validation](validation.md), and [responses](responses.md).

For application structure, read [dependencies](dependencies.md), [resource cleanup and overrides](resources.md), [lifecycle and state](lifecycle.md), and [middleware and CORS](middleware.md).

Publish interactive documentation with [Swagger and OpenAPI](openapi.md). Protect routes with [local authentication](authentication.md), [external OAuth2/OIDC login](oidc.md), and [authorization scopes](authorization.md).

Transport guides cover [uploads](uploads.md), [streaming and SSE](streaming.md), and [WebSockets](websockets.md). Finish with [testing](testing.md), [TLS and deployment](../deploy/web-server.md), and the [multifile service example](example.md).

## Guides in this section

<!-- pages -->

## Requirements

HTTP servers need a native Dream target and the `dream-host-net` capability, and local authentication also needs `dream-host-crypto`. Server calls are rejected when you compile to WebAssembly, while the HTTP client stays available through `system.net`.

Applications mount routers explicitly, and importing a file does not publish its handlers. Two apps can mount the same router with different prefixes and keep separate middleware, state, documentation, dependency overrides, and shutdown.
