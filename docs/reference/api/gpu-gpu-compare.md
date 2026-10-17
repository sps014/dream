# GpuCompare

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuCompare`

A comparison between a new value and a stored one, used by depth tests and comparison samplers. The test passes when `new <op> stored` holds.

```dream
public enum GpuCompare
```

## `Never`

Never passes.

```dream
Never = 0
```

## `Less`

Passes when the new value is smaller.

```dream
Less = 1
```

## `LessEqual`

Passes when the new value is smaller or equal.

```dream
LessEqual = 2
```

## `Equal`

Passes when the values are equal.

```dream
Equal = 3
```

## `GreaterEqual`

Passes when the new value is greater or equal.

```dream
GreaterEqual = 4
```

## `Greater`

Passes when the new value is greater.

```dream
Greater = 5
```

## `NotEqual`

Passes when the values differ.

```dream
NotEqual = 6
```

## `Always`

Always passes.

```dream
Always = 7
```
