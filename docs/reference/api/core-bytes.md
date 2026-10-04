# Bytes

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Bytes`

```dream
public static class Bytes
```

## `toWireString`

Encodes `bytes` as a `string` with one codepoint (0-255) per byte - lossless, since every byte value is a valid standalone Unicode codepoint, and the inverse of `fromWireString`. Used to carry an arbitrary byte buffer over a channel that only understands `string`s (the `Task` wire), without needing a second, binary-native transport.

```dream
public static fun toWireString(borrow bytes: byte[]): string
```

## `fromWireString`

The inverse of `toWireString`.

```dream
public static fun fromWireString(borrow s: string): byte[]
```
