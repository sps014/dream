# Web App

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../stdlib/webapi.md).

## `public class WebApp`

A native HTTP application whose routes and entry points are generated from attributes.

```dream
public class WebApp
```

## `public static fun use`

Appends a runtime onion middleware layer.

```dream
public static fun use(middleware: Middleware): void
```

## `public static fun shutdown`

Stops accepting connections and cancels pending request operations.

```dream
public static fun shutdown(): void
```

## `public static async fun wait`

Waits for the server loop, optionally shutting it down on cancellation.

```dream
public static async fun wait(token: Option<CancellationToken> = Option.None): void
```
