# Bytes

**Import:** `import system.core;`

## `Bytes`

Copies unmanaged values to and from byte arrays. Only types with no reference fields can be copied this way.

```dream
public static class Bytes
```

## `of`

Copy the raw bytes of blittable `value` into a fresh `byte[]` (one element per byte).

```dream
public static extern fun of<T: unmanaged>(value: T): byte[]
```

## `to`

Reconstruct a blittable `T` from a `byte[]` buffer produced by `Bytes.of<T>`.

```dream
public static extern fun to<T: unmanaged>(borrow bytes: byte[]): T
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

Encodes a value for transport to a task. Strings are passed as text; other values must be unmanaged.

```dream
public static extern fun toWire<T>(borrow value: T): string
```

## `fromWire`

The inverse of `toWire`.

```dream
public static extern fun fromWire<T>(borrow text: string): T
```
