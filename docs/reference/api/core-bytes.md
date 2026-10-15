# Bytes

**Import:** `import system.core;`

## `Bytes`

`Bytes` - raw byte-blit conversions between a blittable value and a `byte[]` buffer, backing two compiler intrinsics. These underpin the value-copy `Task` boundary: a value is flattened to a byte buffer on one side and rebuilt from an identical buffer on the other, with no marshalling.  Only blittable values (`T : unmanaged`, recursively free of reference fields) are safe to copy this way - a raw byte copy of a heap reference would carry a pointer that is meaningless in another memory space.

```dream
public static class Bytes
```

## `of`

Copy the raw bytes of blittable `value` into a fresh `byte[]` (one element per byte).

```dream
public static extern fun of<T : unmanaged>(value: T): byte[]
```

## `to`

Reconstruct a blittable `T` from a `byte[]` buffer produced by `Bytes.of<T>`.

```dream
public static extern fun to<T : unmanaged>(borrow bytes: byte[]): T
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

## `toWire`

Encodes a `Task`-safe value of type `T` as the `string` the worker wire carries: identity when `T` is `string` itself, otherwise a raw byte-blit (`T` must be `unmanaged`) re-encoded via `toWireString`. Used by `Task.spawn`/`TaskPool.map`/ `TaskPool.dispatch` to marshal an arbitrary wire value without a second wire format.

```dream
public static extern fun toWire<T>(borrow value: T): string
```

## `fromWire`

The inverse of `toWire`.

```dream
public static extern fun fromWire<T>(borrow text: string): T
```
