# Vector

**Import:** `import system.simd;`

Read the [usage guide](../../library/text/simd.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Vector`

Portable 16-byte SIMD vector (`v128`). `T` must be `byte`, `int`, `long`, `float`, or `double`.

```dream
public struct Vector<T: unmanaged>
```

## `lane_count`

Number of elements held by this vector type.

```dream
public static extern fun lane_count(): int
```

## `constructor`

Splats `value` into every lane.

```dream
public constructor(value: T)
```

## `splat`

Splats `value` into every lane.

```dream
public static fun splat(value: T): Vector<T>
```

## `splat_raw`

Creates a vector with every lane equal to `value`.

```dream
public static extern fun splat_raw(value: T): Vector<T>
```

## `load`

Loads `Vector<T>.count` consecutive elements from `src` starting at `offset`.

```dream
public static fun load(borrow src: T[], offset: int): Vector<T>
```

## `load_raw`

Loads a vector from `src` starting at `offset`; the range must contain enough elements.

```dream
public static extern fun load_raw(borrow src: T[], offset: int): Vector<T>
```

## `store`

Writes this vector's lanes into `dest` starting at `offset`.

```dream
public fun store(borrow dest: T[], offset: int): void
```

## `store_raw`

Stores all vector lanes into `dest` starting at `offset`; the range must contain enough elements.

```dream
public static extern fun store_raw(v: Vector<T>, borrow dest: T[], offset: int): void
```

## `lane`

Returns lane `i` (`0 .. count`).

```dream
public fun lane(i: int): T
```

## `with_lane`

Returns a copy with lane `i` replaced by `value`.

```dream
public fun with_lane(i: int, value: T): Vector<T>
```

## `bin_add`

Adds corresponding lanes of the two vectors.

```dream
public static extern fun bin_add(a: Vector<T>, b: Vector<T>): Vector<T>
```

## `bin_sub`

Subtracts corresponding lanes of the two vectors.

```dream
public static extern fun bin_sub(a: Vector<T>, b: Vector<T>): Vector<T>
```

## `bin_mul`

Multiplies corresponding lanes of the two vectors.

```dream
public static extern fun bin_mul(a: Vector<T>, b: Vector<T>): Vector<T>
```

## `min`

Lane-wise minimum of `a` and `b`.

```dream
public static fun min(a: Vector<T>, b: Vector<T>): Vector<T>
```

## `bin_min`

Selects the smaller value in each pair of lanes.

```dream
public static extern fun bin_min(a: Vector<T>, b: Vector<T>): Vector<T>
```

## `max`

Lane-wise maximum of `a` and `b`.

```dream
public static fun max(a: Vector<T>, b: Vector<T>): Vector<T>
```

## `bin_max`

Selects the larger value in each pair of lanes.

```dream
public static extern fun bin_max(a: Vector<T>, b: Vector<T>): Vector<T>
```

## `sum`

Horizontal sum of every lane.

```dream
public fun sum(): T
```

## `reduce_sum`

Adds all vector lanes and returns the sum.

```dream
public static extern fun reduce_sum(v: Vector<T>): T
```
