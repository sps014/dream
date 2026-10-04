# NetWireReader

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class NetWireReader`

Sequential reader over a `char[]` wire payload shared by the TCP/WebSocket host functions: a tag line, then raw bytes for the remainder (mirrors `system.process`'s `ProcessWireReader`). Each `char` element of the payload is one raw byte (0-255).

```dream
public class NetWireReader
```

## `constructor`

```dream
public constructor(data: char[])
```

## `read_line`

Reads and consumes everything up to the next '\n' (exclusive), then skips the '\n'.

```dream
public fun read_line(): string
```

## `read_bytes`

Reads and consumes the next `count` raw bytes (clamped to what remains).

```dream
public fun read_bytes(count: int): byte[]
```

## `read_remaining_bytes`

Reads and consumes every remaining byte.

```dream
public fun read_remaining_bytes(): byte[]
```

## `read_remaining_text`

Reads and consumes every remaining byte as UTF-8 text.

```dream
public fun read_remaining_text(): string
```
