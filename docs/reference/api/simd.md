# Vector

**Import:** `import system.simd;`

Read the [usage guide](../stdlib/simd.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct Vector<T : unmanaged>`

Portable 16-byte SIMD vector (`v128`). `T` must be `byte`, `int`, `long`, `float`, or `double`.

```dream
public struct Vector<T : unmanaged>
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

## `load`

Loads `Vector<T>.count` consecutive elements from `src` starting at `offset`.

```dream
public static fun load(borrow src: T[], offset: int): Vector<T>
```

## `store`

Writes this vector's lanes into `dest` starting at `offset`.

```dream
public fun store(borrow dest: T[], offset: int): void
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

## `min`

Lane-wise minimum of `a` and `b`.

```dream
public static fun min(a: Vector<T>, b: Vector<T>): Vector<T>
```

## `max`

Lane-wise maximum of `a` and `b`.

```dream
public static fun max(a: Vector<T>, b: Vector<T>): Vector<T>
```

## `sum`

Horizontal sum of every lane.

```dream
public fun sum(): T
```
