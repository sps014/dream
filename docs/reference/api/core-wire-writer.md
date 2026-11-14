# Wire Writer

**Import:** `import system.core;`

## `WireWriter`

Builds a little-endian record: fixed-width integers and strings as a 32-bit UTF-8 byte length followed by the bytes.

```dream
public class WireWriter
```

## `constructor`

Creates an empty record with room for `capacity` bytes before it grows.

```dream
public constructor(capacity: int)
```

## `u8`

Appends the low 8 bits of `value`.

```dream
public fun u8(value: int): void
```

## `i32`

Appends `value` as four bytes.

```dream
public fun i32(value: int): void
```

## `i64`

Appends `value` as eight bytes.

```dream
public fun i64(value: long): void
```

## `text`

Appends `value` as a 32-bit byte length and its UTF-8 bytes. Unpaired surrogates become U+FFFD.

```dream
public fun text(borrow value: string): void
```

## `finish`

The finished record.

```dream
public fun finish(): byte[]
```
