# Stdin, Stdout, Stderr

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Stdin`

Standard input. Obtain via `System.stdin`.

```dream
public class Stdin
```

## `read_line`

Reads a line from standard input.

```dream
public fun read_line(): string
```

## `read_key`

Reads one character from standard input.

```dream
public fun read_key(): char
```

## `Stdout`

Standard output. Obtain via `System.stdout`.

```dream
public class Stdout
```

## `write_text`

Writes text without adding a newline.

```dream
public fun write_text(borrow text: string): void
```

## `write_line`

Writes text followed by a newline.

```dream
public fun write_line(borrow text: string): void
```

## `Stderr`

Standard error. Obtain via `System.stderr`.

```dream
public class Stderr
```

## `write_text`

Writes text without adding a newline.

```dream
public fun write_text(borrow text: string): void
```

## `write_line`

Writes text followed by a newline.

```dream
public fun write_line(borrow text: string): void
```

## `stdin`

Standard input stream.

```dream
public static get stdin(): Stdin
```

## `stdout`

Standard output stream.

```dream
public static get stdout(): Stdout
```

## `stderr`

Standard error stream.

```dream
public static get stderr(): Stderr
```
