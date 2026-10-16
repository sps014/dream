# Csv

**Import:** `import system.csv;`

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
public fun write_record(fields: string[]): Result<bool, CsvError>
```

## `flush`

Flushes the borrowed destination and reports underlying failures.

```dream
public fun flush(): Result<bool, CsvError>
```
