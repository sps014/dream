# system.webapi: API lookup

Native server APIs. Read the [Web API guide](../stdlib/webapi.md) for examples, generated routes, lifecycle methods, streaming and target availability.

- [Auth](webapi-auth.md)
- [Cors](webapi-cors.md)
- [Http Incoming](webapi-http-incoming.md)
- [Http Outgoing](webapi-http-outgoing.md)
- [Http Status](webapi-http-status.md)
- [Middleware](webapi-middleware.md)
- [Server Ws](webapi-server-ws.md)
- [Sse Event](webapi-sse-event.md)
- [Uploaded File](webapi-uploaded-file.md)
- [Web App](webapi-web-app.md)
- [Web App Options](webapi-web-app-options.md)
- [Attributes](webapi-webapi-attributes.md)

## Generated WebApp entry points

`system.webapi.derive` adds these methods after inspecting the routes:

```dream
public static async fun listen(host: string, port: int, token: Option<CancellationToken> = Option.None): Result<int, IoError>
public static async fun listen_with(host: string, port: int, options: WebAppOptions, token: Option<CancellationToken> = Option.None): Result<int, IoError>
public static async fun run(host: string, port: int, token: Option<CancellationToken> = Option.None): Result<bool, IoError>
public static async fun run_with(host: string, port: int, options: WebAppOptions, token: Option<CancellationToken> = Option.None): Result<bool, IoError>
```

These methods are native only. Listen returns the bound port, including the selected ephemeral port when passed zero. Run listens and waits until shutdown or cancellation.

[Generator entry point](webapi-derive-webapi-derive.md).
