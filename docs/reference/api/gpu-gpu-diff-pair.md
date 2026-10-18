# GpuDiffPair

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuDiffPair`

A value together with its derivative, the argument type of `GpuDiff.forward` and `GpuDiff.backward`.  For `forward`, `derivative` is the direction to differentiate in: `1` for the input you differentiate by and `0` for the rest. After `backward`, each pair's `derivative` holds the gradient of the result with respect to that input. `T` is `float`, `float16`, a GPU vector, or a struct whose fields are all differentiable.

```dream
public struct GpuDiffPair<T>
```

## `constructor`

A pair of `value` and `derivative`.

```dream
public constructor(value: T, derivative: T)
```

## `constructor`

A pair whose derivative is zero, for inputs that are held constant.

```dream
public constructor(value: T)
```

## `value`

The value.

```dream
public extern fun value(): T
```

## `derivative`

The derivative.

```dream
public extern fun derivative(): T
```
