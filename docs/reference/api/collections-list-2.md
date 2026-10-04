# List

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections/list.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](collections-list.md)

## `clear`

```dream
public fun clear(): void where T : unmanaged
```

## `insert`

Inserts `value` at `index`, shifting later elements right. Returns false if `index` is out of range (index may equal `size()` to append).

```dream
public fun insert(index: int, value: T): bool
```

## `insert`

Unmanaged: one `memory.copy` shift (no Span wrappers).

```dream
public fun insert(index: int, value: T): bool where T : unmanaged
```

## `remove_at`

Removes the element at `index`, shifting later elements left. Returns true on success, or false if `index` is out of range (the list is left unchanged in that case).

```dream
public fun remove_at(index: int): bool
```

## `remove`

Removes the first occurrence of `value` (by equality). Returns true when something was removed.

```dream
public fun remove(borrow value: T): bool
```

## `iterator`

Enumerator for `for (let x in list)`. Returns a fresh cursor over the current elements.

```dream
public fun iterator(): ListIterator<T>
```

## `sort_by`

Sorts the list in place with the supplied comparator.

```dream
public fun sort_by(borrow cmp: fun(T, T): int): void
```

## `sort`

Sorts the list in place into ascending order using each element's `compare` method. Only attached when `T : Comparable<T>` (via `where`).

```dream
public fun sort(): void where T : Comparable<T>
```

## `binary_search`

Binary search over a list that is already sorted ascending (by `compare`). Returns the index of a matching element as `Some(index)`, or `None` if `value` is not present. O(log n). Only attached when `T : Comparable<T>` (via `where`).

```dream
public fun binary_search(borrow value: T): Option<int> where T : Comparable<T>
```

## `to_string`

```dream
public override fun to_string(): string
```
