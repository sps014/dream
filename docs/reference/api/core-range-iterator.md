# Range Iterator

**Import:** `import system.core;`

Call `range.iterator()` to create an independent cursor. Call `next()` until it returns `Option.None`; subsequent calls also return `None`.

## `RangeIterator: Iterator<int>`

A cursor over an exclusive integer range, moving toward its end.

```dream
public class RangeIterator: Iterator<int>
```

## `next`

Returns the next integer, or `None` after reaching the excluded end.

```dream
public fun next(): Option<int>
```
