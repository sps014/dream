# ArgumentError

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ArgumentError`

Invalid CLI / environment usage implementing `Error`.

```dream
public class ArgumentError: Error
```

## `constructor`

Builds an error with a stable `code` string and human-readable `message`.

```dream
public constructor(code: string, message: string)
```

## `message`

Human-readable explanation of the invalid command-line argument, suitable for logs and error output.

```dream
public fun message(): string
```

## `code`

Stable machine-readable code (e.g. `EINVAL`).

```dream
public fun code(): string
```

## `invalid`

Convenience constructor for invalid-argument failures.

```dream
public static fun invalid(message: string): ArgumentError
```
