# Web Router

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public class WebEndpoint`

A compiled handler, path and its generated documentation.

```dream
public class WebEndpoint
```

## `public constructor`

```dream
public constructor(method: string, path: string, handler: fun(RequestContext, Map<string, string>): Future<HttpOutgoing>, paths: string, websocket: bool = false, components: string = "{}")
```

## `public class WebRouter`

Immutable generated route group; mount it explicitly on an application.

```dream
public class WebRouter
```

## `public constructor`

```dream
public constructor(routes: List<WebEndpoint>)
```
