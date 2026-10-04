# Stdin, Stdout, Stderr

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Stdin`

Standard input. Obtain via `System.stdin`.

```dream
public class Stdin
```

## `read_line`

```dream
public fun read_line(): string
```

## `read_key`

```dream
public fun read_key(): char
```

## `class Stdout`

Standard output. Obtain via `System.stdout`.

```dream
public class Stdout
```

## `write_text`

```dream
public fun write_text(borrow text: string): void
```

## `write_line`

```dream
public fun write_line(borrow text: string): void
```

## `class Stderr`

Standard error. Obtain via `System.stderr`.

```dream
public class Stderr
```

## `write_text`

```dream
public fun write_text(borrow text: string): void
```

## `write_line`

```dream
public fun write_line(borrow text: string): void
```

## `stdin`

```dream
public static get stdin(): Stdin
```

## `stdout`

```dream
public static get stdout(): Stdout
```

## `stderr`

```dream
public static get stderr(): Stderr
```
