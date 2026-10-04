# Collection Tuning

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `sequence_min_capacity`

Smallest backing-buffer capacity `Array`/`List` will grow to (also their capacity floor when constructed with a smaller explicit capacity).

```dream
public fun sequence_min_capacity(): int
```

## `hash_min_capacity`

Smallest hash-table backing-array capacity (already a power of 2). `Map`/`Set.clear` keep the current capacity and reuse buffers rather than shrinking back to this floor.

```dream
public fun hash_min_capacity(): int
```

## `hash_epoch_initial`

First occupancy stamp stored in `Map`/`Set` probe slots (`0` remains empty).

```dream
public fun hash_epoch_initial(): int
```

## `hash_load_factor_num`

`Set`/`Map` grow when `(used + 1) * hash_load_factor_denom() >= capacity * hash_load_factor_num()`, i.e. before the table would exceed 3/4 full, keeping the average probe length short.

```dream
public fun hash_load_factor_num(): int
```

## `hash_load_factor_denom`

```dream
public fun hash_load_factor_denom(): int
```
