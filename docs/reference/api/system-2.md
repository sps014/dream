# System

**Import:** `import system;`

## `set_cwd`

Sets the current working directory.

```dream
public static fun set_cwd(borrow path: string): Result<bool, IoError>
```

## `clear`

Clears the terminal screen.

```dream
public static fun clear(): void
```

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
