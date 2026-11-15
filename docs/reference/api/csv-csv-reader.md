# CsvReader

**Import:** `import system.csv;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `CsvReader`

Bounded-memory CSV reader over a borrowed byte stream, with strict quoting and UTF-8 validation.

```dream
public class CsvReader
```

## `constructor`

Borrows source; record limits count encoded bytes including embedded newlines.

```dream
public constructor(source: Reader, delimiter: char = ',', max_record_bytes: int = 1048576)
```

## `read_record`

Returns a record or None at EOF; malformed input permanently fails this reader.

```dream
public fun read_record(): Result<Option<string[]>, CsvError>
```
