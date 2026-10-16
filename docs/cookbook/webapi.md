# Serve a JSON API

Start with the complete [first application](../web-apis/first-application.md), then add [validated models](../web-apis/validation.md) and [dependencies](../web-apis/dependencies.md).

Run the minimal repository sample:

```sh
dream run sample/webapi/app.dream
```

It serves `/hello/{name}` and an SSE `/events` stream. Open `/docs` for interactive requests.

The [complete service example](../web-apis/example.md) adds local authentication, scopes, CORS, request validation, multiple source files, and HTTP tests. The [Web APIs section](../web-apis/index.md) explains configuration and failure behavior in detail.
