# AsyncIterator

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AsyncIterator`

A cursor whose next value may require asynchronous work. Close it when finished.

```dream
public interface AsyncIterator<T>
```

## `next`

Returns the next value, or `None` when the sequence has ended.

```dream
async unique fun next(): Option<T>
```

## `close`

Ends iteration and releases resources held by the cursor.

```dream
unique fun close(): void
```
