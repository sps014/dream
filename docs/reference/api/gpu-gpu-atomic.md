# GpuAtomic

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuAtomic`

Atomic read-modify-write operations on buffer elements, safe when many threads update the same element at once. Each returns the element's value from before the operation. Integer forms work on `GpuBuffer<int>` and `GpuBuffer<uint>`; `add_float` on `GpuBuffer<float>`.

```dream
public static class GpuAtomic
```

## `add`

Adds `value` to element `index`.

```dream
public static extern fun add<T>(buffer: GpuBuffer<T>, index: int, value: T): T
```

## `add_float`

Adds `value` to the float element `index`. WebGPU has no float atomics, so there it is emulated with a compare-and-swap loop.

```dream
public static extern fun add_float(buffer: GpuBuffer<float>, index: int, value: float): float
```

## `sub`

Subtracts `value` from element `index`.

```dream
public static extern fun sub<T>(buffer: GpuBuffer<T>, index: int, value: T): T
```

## `min`

Stores the smaller of element `index` and `value`.

```dream
public static extern fun min<T>(buffer: GpuBuffer<T>, index: int, value: T): T
```

## `max`

Stores the larger of element `index` and `value`.

```dream
public static extern fun max<T>(buffer: GpuBuffer<T>, index: int, value: T): T
```

## `and`

Stores the bitwise AND of element `index` and `value`.

```dream
public static extern fun and<T>(buffer: GpuBuffer<T>, index: int, value: T): T
```

## `or`

Stores the bitwise OR of element `index` and `value`.

```dream
public static extern fun or<T>(buffer: GpuBuffer<T>, index: int, value: T): T
```

## `xor`

Stores the bitwise exclusive OR of element `index` and `value`.

```dream
public static extern fun xor<T>(buffer: GpuBuffer<T>, index: int, value: T): T
```

## `exchange`

Stores `value` in element `index`.

```dream
public static extern fun exchange<T>(buffer: GpuBuffer<T>, index: int, value: T): T
```

## `compare_exchange`

Stores `value` in element `index` only if the element equals `expected`.

```dream
public static extern fun compare_exchange<T>(
    buffer: GpuBuffer<T>,
    index: int,
    expected: T,
    value: T
): T
```
