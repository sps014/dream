# Bytes

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Bytes`

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

Encodes a `Task`-safe value of type `T` as the `string` the worker wire carries: identity when `T` is `string` itself, otherwise a raw byte-blit (`T` must be `unmanaged`) re-encoded via `toWireString`. Used by `Task.spawn`/`Task.map`/ `TaskPool.dispatch` to marshal an arbitrary wire value without a second wire format.

```dream
public static extern fun toWire<T>(borrow value: T): string
```

## `fromWire`

The inverse of `toWire`.

```dream
public static extern fun fromWire<T>(borrow text: string): T
```
