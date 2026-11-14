# Wire Reader

**Import:** `import system.core;`

## `WireReader`

Reads a little-endian record in the `WireWriter` layout. A read past the end or a malformed length yields a zero value and clears `ok`, so callers check `ok` once after decoding every field.

```dream
public class WireReader
```

## `ok`

False once any read ran past the end of the record.

```dream
public ok: bool
```

## `constructor`

Reads `bytes` from byte offset `start`.

```dream
public constructor(bytes: byte[], start: int)
```

## `at_end`

Whether every byte of the record has been read.

```dream
public fun at_end(): bool
```

## `u8`

Reads one unsigned byte.

```dream
public fun u8(): int
```

## `i32`

Reads four bytes as a signed 32-bit integer.

```dream
public fun i32(): int
```

## `i64`

Reads eight bytes as a signed 64-bit integer.

```dream
public fun i64(): long
```

## `bytes_field`

Reads a 32-bit byte length and that many bytes.

```dream
public fun bytes_field(): byte[]
```

## `text`

Reads a 32-bit byte length and that many UTF-8 bytes as a string. Malformed sequences become U+FFFD.

```dream
public fun text(): string
```
