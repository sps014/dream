# Character view search

No import is needed.

Read the [span guide](../../language/types/spans.md) for usage and backing-storage semantics.

## `contains`

Returns whether the collection contains the requested key or value.

```dream
public fun contains(borrow sub: string): bool
```

## `index_of`

Returns the first matching index, or `None` when no match exists.

```dream
public fun index_of(c: char, from: int): Option<int>
```

## `index_of`

Returns the first matching index, or `None` when no match exists.

```dream
public fun index_of(borrow sub: string): Option<int>
```

## `index_of`

Returns the first matching index, or `None` when no match exists.

```dream
public fun index_of(borrow sub: string, from: int): Option<int>
```

## `last_index_of`

Returns the last matching index, or `None` when no match exists.

```dream
public fun last_index_of(c: char): Option<int>
```

## `last_index_of`

Returns the last matching index, or `None` when no match exists.

```dream
public fun last_index_of(borrow sub: string): Option<int>
```
