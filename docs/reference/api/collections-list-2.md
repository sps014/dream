# List

**Import:** `import system.collections;`

## `clear`

Removes all elements and releases the references they held. Capacity is unchanged.

```dream
public fun clear(): void where T: unmanaged
```

## `insert`

Inserts `value` at `index`, shifting later elements right. Returns false if the index is outside 0 through length.

```dream
public fun insert(index: int, value: T): bool
```

## `insert`

Inserts `value` at `index`, shifting later elements right. Returns false if the index is outside 0 through length.

```dream
public fun insert(index: int, value: T): bool where T: unmanaged
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

## `as_span`

A view of the current elements, writable in place. The span holds the backing array it was taken from: after the list grows (or shrinks and regrows) it keeps showing that old array, so take a fresh span after structural changes.

```dream
public fun as_span(): Span<T>
```

## `as_read_only_span`

A read-only view of the current elements, with the same staleness rule as `as_span`.

```dream
public fun as_read_only_span(): ReadOnlySpan<T>
```

## `sort_by`

Sorts the list in place with the supplied comparator.

```dream
public fun sort_by(borrow cmp: fun(T, T): int): void
```

## `sort`

Sorts the list in place into ascending order using each element's `compare` method. Only attached when `T : Comparable<T>` (via `where`).

```dream
public fun sort(): void where T: Comparable<T>
```

## `binary_search`

Binary search over a list that is already sorted ascending (by `compare`). Returns the index of a matching element as `Some(index)`, or `None` if `value` is not present. O(log n). Only attached when `T : Comparable<T>` (via `where`).

```dream
public fun binary_search(borrow value: T): Option<int> where T: Comparable<T>
```

## `to_string`

Returns a text representation of this value.

```dream
public override fun to_string(): string
```
