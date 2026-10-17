# System

**Import:** `import system.io;`

Read the [usage guide](../../library/io/streams.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `stdin`

Standard input as a byte stream; reading does not close the process descriptor.

```dream
public static get stdin(): Stdin
```

## `stdout`

Standard output as a byte stream; flushing does not close it.

```dream
public static get stdout(): Stdout
```

## `stderr`

Standard error as a byte stream; flushing does not close it.

```dream
public static get stderr(): Stderr
```
