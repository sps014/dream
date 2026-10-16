# Web Router

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-server/index.md) for runnable examples, lifecycle, and error behavior.

## `WebEndpoint`

A compiled handler, path and its generated documentation.

```dream
public class WebEndpoint
```

## `constructor`

Creates a `WebEndpoint` with `method`, `path`, `handler`, `paths`, `websocket`, `components`.

```dream
public constructor(
    method: string,
    path: string,
    handler: fun(RequestContext, Map<string, string>): Future<HttpOutgoing>,
    paths: string,
    websocket: bool = false,
    components: string = "{}"
)
```

## `WebRouter`

Immutable generated route group; mount it explicitly on an application.

```dream
public class WebRouter
```

## `constructor`

Creates a `WebRouter` with `routes`.

```dream
public constructor(routes: List<WebEndpoint>)
```
