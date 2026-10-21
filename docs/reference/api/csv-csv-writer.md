# CsvWriter

**Import:** `import system.csv;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `CsvWriter`

CSV writer over a borrowed byte stream; records use CRLF and standard doubled-quote escaping.

```dream
public class CsvWriter
```

## `constructor`

Borrows destination; callers retain responsibility for closure and explicit flushing.

```dream
public constructor(destination: Writer, delimiter: char = ',', max_record_bytes: int = 1048576)
```

## `write_record`

Writes exactly one record, retrying partial writes and reporting quoting/size/I/O failures.

```dream
public fun write_record(fields: string[]): Result<Unit, CsvError>
```

## `flush`

Flushes the borrowed destination and reports underlying failures.

```dream
public fun flush(): Result<Unit, CsvError>
```
