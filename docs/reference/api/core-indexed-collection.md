# IndexedCollection

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `IndexedCollection`

Ordered, indexable sequences (`List`). Extends `Collection` so implementers are usable wherever a `Collection` is expected without re-listing `size` / `iterator`.

```dream
public interface IndexedCollection<T>: Collection<T>
```

## `this`

Accesses the element at `index`; an out-of-range index panics.

```dream
fun this[index: int]: T
```

## `first`

Element at index 0, or `None` when empty.

```dream
public fun first(): Option<T>
```

## `last`

Last element, or `None` when empty.

```dream
public fun last(): Option<T>
```

## `find_index`

Index of the first element matching `pred`, or `None`.

```dream
public fun find_index(borrow pred: fun(T): bool): Option<int>
```
