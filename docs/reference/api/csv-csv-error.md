# CsvError

**Import:** `import system.csv;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `CsvError`

CSV syntax, encoding, size-limit, or underlying I/O failure with byte-oriented provenance.

```dream
public class CsvError: Error
```

## `record`

One-based logical record number associated with the error.

```dream
public record: long
```

## `offset`

Zero-based absolute byte offset reached before the failure.

```dream
public offset: long
```

## `constructor`

Creates a CSV diagnostic; offsets count bytes rather than UTF-16 code units.

```dream
public constructor(record: long, offset: long, message: string)
```

## `code`

Stable CSV failure code.

```dream
public fun code(): string
```

## `message`

Human-readable syntax, limit, adapter, or I/O failure.

```dream
public fun message(): string
```
