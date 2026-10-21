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
