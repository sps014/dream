# Buffered Reader

**Import:** `import system.io;`

## `public class BufferedReader : Reader`

Reads ahead from a borrowed reader without closing it; use this wrapper for positioning while buffered bytes remain, or recover them before accessing the source directly.

```dream
public class BufferedReader : Reader
```

## `public constructor`

Wraps `source` with an 8 KiB default buffer; nonpositive sizes fail on reads.

```dream
public constructor(source: Reader, buffer_size: int = 8192)
```

## `public fun read`

Returns up to `max_bytes` from buffered input, refilling as needed; may return fewer. Zero is a no-op, negative sizes are invalid, and empty success for positive sizes is EOF.

```dream
public fun read(max_bytes: int): Result<byte[], IoError>
```

## `public fun read_line`

Returns a line without LF or CRLF, preserving standalone CR and final unterminated lines. Returns `None` at EOF; `ELIMIT` consumes the prefix and exceeding byte when the pre-LF byte count exceeds `max_bytes` (default 1 MiB, including any CR).

```dream
public fun read_line(max_bytes: int = 1048576): Result<Option<string>, IoError>
```

## `public fun seek`

Seeks the source and discards buffered input only after success. Unsupported or failed seeks preserve unread buffered bytes.

```dream
public fun seek(position: long): Result<bool, IoError>
```

## `public fun tell`

Returns the source position minus unread buffered bytes; propagates positioning errors.

```dream
public fun tell(): Result<long, IoError>
```

## `public fun take_buffered`

Copies and removes unread buffered bytes without repositioning the source. Recover these bytes before bypassing this wrapper to access the source directly.

```dream
public fun take_buffered(): byte[]
```
