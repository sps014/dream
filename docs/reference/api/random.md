# Random

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Random`

Seedable xorshift32 PRNG.

```dream
public class Random
```

## `constructor`

Creates a generator from `seed` (`0` is remapped to `1` so the state is never zero).

```dream
public constructor(seed: uint)
```

## `next_u32`

Advances the generator and returns the next 32-bit value.

```dream
public fun next_u32(): uint
```

## `next_int`

Uniform integer in `[0, bound)` when `bound > 0`, otherwise `0`.

```dream
public fun next_int(bound: int): int
```

## `next_double`

Uniform value in `[0.0, 1.0)` at full 32-bit resolution (no quantization or modulo bias).

```dream
public fun next_double(): double
```

## `next_bool`

True or false with equal probability.

```dream
public fun next_bool(): bool
```

## `next_long`

Uniform 64-bit value from two independent 32-bit draws, composed exactly (may be negative when viewed as signed).

```dream
public fun next_long(): long
```

## `next_bytes`

```dream
public fun next_bytes(n: int): byte[]
```

## `choice`

```dream
public fun choice<T>(list: List<T>): Option<T>
```

## `shuffle`

```dream
public fun shuffle<T>(list: List<T>): void
```
