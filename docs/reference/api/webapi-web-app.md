# Web App

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public class WebApp`

An isolated native HTTP application with explicitly mounted routers.

```dream
public class WebApp
```

## `public state`

```dream
public state: Map<string, string>
```

## `public constructor`

```dream
public constructor()
```

## `public constructor`

```dream
public constructor(options: WebAppOptions)
```

## `public fun use`

Appends an outer-to-inner middleware layer before startup.

```dream
public fun use(middleware: Middleware): Result<bool, IoError>
```

## `public fun include_router`

Mounts a router with an optional additional path prefix.

```dream
public fun include_router(router: WebRouter, prefix: string = ""): Result<bool, IoError>
```

## `public fun override_dependencies`

Installs generated dependency replacements before startup.

```dream
public fun override_dependencies(overrides: DependencyOverrides): Result<bool, IoError>
```

## `public fun lifespan`

Pairs one startup acquisition with its shutdown release; completed pairs unwind in reverse order.

```dream
public fun lifespan(start: fun(WebApp): Future<Result<bool, IoError>>, stop: fun(WebApp): Future<void>): Result<bool, IoError>
```

## `public fun openapi`

Produces documentation for this application's mounted routes only.

```dream
public fun openapi(): string
```
