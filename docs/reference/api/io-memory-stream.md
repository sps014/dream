# Memory Stream

**Import:** `import system.io;`

## `public class MemoryStream : Reader, Writer, AsyncReader, AsyncWriter`

Seekable in-memory byte storage; closing rejects operations while retaining its data.

```dream
public class MemoryStream : Reader, Writer, AsyncReader, AsyncWriter
```

## `public constructor`

Creates an empty stream positioned at byte zero.

```dream
public constructor()
```

## `public constructor`

Copies `bytes` into independent storage and starts at byte zero.

```dream
public constructor(bytes: byte[])
```

## `public get position`

Last cursor position in bytes; remains readable after closure.

```dream
public get position(): long
```

## `public fun tell`

Returns the current byte position, or a closed-resource error.

```dream
public fun tell(): Result<long, IoError>
```

## `public get length`

Stored byte count; remains readable after closure.

```dream
public get length(): long
```

## `public fun read`

Reads up to the requested size and advances the cursor; empty success indicates EOF. Negative sizes and reads after closure return errors.

```dream
public fun read(max_bytes: int): Result<byte[], IoError>
```

## `public fun write`

Overwrites or appends bytes at the cursor and returns their count. A nonempty write beyond EOF fills the gap with zeros; size is limited to `int` range.

```dream
public fun write(bytes: byte[]): Result<int, IoError>
```

## `public fun seek`

Sets a byte position from zero through the maximum `int`, including beyond EOF. Invalid positions or closure return errors without changing the cursor.

```dream
public fun seek(position: long): Result<bool, IoError>
```

## `public fun to_bytes`

Copies all stored bytes independently of the cursor; returns an error after closure.

```dream
public fun to_bytes(): Result<byte[], IoError>
```

## `public unique fun flush`

Validates that the stream is open; memory storage has no pending output.

```dream
public unique fun flush(): Result<bool, IoError>
```

## `public fun close`

Closes the stream idempotently; subsequent I/O returns a closed-resource error.

```dream
public fun close(): Result<bool, IoError>
```

## `public async fun read_async`

Checks cancellation, then performs an immediately ready in-memory read.

```dream
public async fun read_async(max_bytes: int, token: Option<CancellationToken> = Option.None): Result<byte[], IoError>
```

## `public async fun write_async`

Checks cancellation, then performs an immediately ready in-memory write.

```dream
public async fun write_async(data: byte[], token: Option<CancellationToken> = Option.None): Result<int, IoError>
```

## `public async unique fun flush_async`

Checks cancellation and validates that the stream is open.

```dream
public async unique fun flush_async(token: Option<CancellationToken> = Option.None): Result<bool, IoError>
```
