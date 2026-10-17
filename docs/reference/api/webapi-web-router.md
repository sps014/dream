# WebRouter

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
