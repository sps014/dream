# App Error

**Import:** `import system.app;`

Read the [usage guide](../../library/system/app.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AppError`

Why an `App` operation was refused.

```dream
public class AppError: Error
```

## `code`

The machine-readable reason. `ESTOPPING` means shutdown already began, so a new `on_stop` cleanup would never run.

```dream
public fun code(): string
```

## `message`

A human-readable description of the problem.

```dream
public fun message(): string
```
