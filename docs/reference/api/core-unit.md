# Unit

**Import:** `import system.core;`

## `Unit`

The success value of an operation that has nothing to return. A fallible action returns `Result<Unit, E>`: `Ok(Unit())` on success and `Err(error)` on failure, so `?` and `switch` work the same as for any other `Result`.

```dream
public struct Unit
```

## `constructor`

Creates the single `Unit` value.

```dream
public constructor()
```
