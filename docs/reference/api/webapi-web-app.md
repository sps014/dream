# Web App

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-server/index.md) for runnable examples, lifecycle, and error behavior.

## `WebApp`

An isolated native HTTP application with explicitly mounted routers.

```dream
public class WebApp
```

## `state`

Values stored for this request by handlers and middleware.

```dream
public state: Map<string, string>
```

## `constructor`

Creates a `WebApp` with default settings.

```dream
public constructor()
```

## `constructor`

Creates a `WebApp` with `options`.

```dream
public constructor(options: WebAppOptions)
```

## `use`

Appends an outer-to-inner middleware layer before startup.

```dream
public fun use(middleware: Middleware): Result<bool, IoError>
```

## `include_router`

Mounts a router with an optional additional path prefix.

```dream
public fun include_router(router: WebRouter, prefix: string = ""): Result<bool, IoError>
```

## `override_dependencies`

Installs generated dependency replacements before startup.

```dream
public fun override_dependencies(overrides: DependencyOverrides): Result<bool, IoError>
```

## `lifespan`

Pairs one startup acquisition with its shutdown release; completed pairs unwind in reverse order.

```dream
public fun lifespan(
    start: fun(WebApp): Future<Result<bool, IoError>>,
    stop: fun(WebApp): Future<void>
): Result<bool, IoError>
```

## `shutdown`

Stops accepting traffic and cancels active transport operations on this app.

```dream
public fun shutdown(): void
```

## `wait`

Drains this application's requests and releases its listener.

```dream
public fun wait(token: Option<CancellationToken> = Option.None): Future<void>
```

## `listen`

Binds this app; port zero selects an ephemeral port.

```dream
public fun listen(
    host: string,
    port: int,
    token: Option<CancellationToken> = Option.None
): Future<Result<int, IoError>>
```

## `run`

Listens and waits until shutdown or cancellation.

```dream
public fun run(
    host: string,
    port: int,
    token: Option<CancellationToken> = Option.None
): Future<Result<bool, IoError>>
```

## `openapi`

Produces documentation for this application's mounted routes only.

```dream
public fun openapi(): string
```
