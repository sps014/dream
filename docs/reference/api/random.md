# Random

**Import:** `import system;`

## `Random`

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

Generates `n` pseudorandom bytes; negative lengths produce an empty array.

```dream
public fun next_bytes(n: int): byte[]
```

## `choice`

Selects a uniformly bounded index from `list`, or returns `None` for an empty list.

```dream
public fun choice<T>(list: List<T>): Option<T>
```

## `shuffle`

Permutes `list` in place using Fisher-Yates swaps and unbiased bounded indices.

```dream
public fun shuffle<T>(list: List<T>): void
```
