# Next

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Next`

Continues the middleware onion toward the route handler.

```dream
public class Next
```

## `constructor`

Creates a `Next` with `step`.

```dream
public constructor(step: fun(): Future<HttpOutgoing>)
```

## `call`

Runs the next step in the middleware pipeline.

```dream
public fun call(): Future<HttpOutgoing>
```
