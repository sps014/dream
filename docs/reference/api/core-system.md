# System

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `System`

Process basics available without an import: stopping with `panic` and reading command-line arguments. `import system;` adds console I/O and the other process services.

```dream
public static class System
```

## `panic`

Stops execution and reports `message`.

```dream
public static extern fun panic(borrow message: string): void
```

## `args`

Command-line arguments, excluding the executable path. Empty strings and embedded newlines are preserved. `fun main(args: string[])` receives the same array.

```dream
public static get args(): string[]
```
