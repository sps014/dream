# System

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](system.md)

## `set_foreground`

Sets the foreground color for subsequent output.

```dream
public static fun set_foreground(color: ConsoleColor): void
```

## `set_background`

Sets the background color for subsequent output.

```dream
public static fun set_background(color: ConsoleColor): void
```

## `reset_color`

Resets console colors to the terminal default.

```dream
public static fun reset_color(): void
```

## `print_colored`

Prints a string in the specified color.

```dream
public static fun print_colored(borrow value: string, color: ConsoleColor): void
```
