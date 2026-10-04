# Collection

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `interface Collection<T>`

Sized collection — the type users pass into APIs that need a count. `for (let x in xs)` works for concrete `@iterator` types and for interface-typed `Collection` / `IndexedCollection` / `Iterator` (via `.iterator()` / `.next()` dispatch). List-returning query helpers (`to_list`, `filter`, `map`, …) live in `system.collections` as `extend Collection<T>` so bootstrap does not depend on `List`.

```dream
public interface Collection<T>
```

## `length`

```dream
get length(): int
```

## `iterator`

```dream
fun iterator(): Iterator<T>
```

## `is_empty`

True when the collection has no elements.

```dream
public fun is_empty(): bool
```

## `all`

True when every element satisfies `pred`.

```dream
public fun all(borrow pred: fun(T): bool): bool
```

## `any`

True when at least one element satisfies `pred`.

```dream
public fun any(borrow pred: fun(T): bool): bool
```

## `none`

True when no element satisfies `pred`.

```dream
public fun none(borrow pred: fun(T): bool): bool
```

## `count_where`

Counts elements for which `pred` is true.

```dream
public fun count_where(borrow pred: fun(T): bool): int
```

## `find_where`

First element for which `pred` is true, or `None`.

```dream
public fun find_where(borrow pred: fun(T): bool): Option<T>
```

## `for_each`

Invokes `action` for each element in iteration order.

```dream
public fun for_each(borrow action: fun(T): void): void
```
