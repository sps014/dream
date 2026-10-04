# StringIterator

**Import:** `import system.text;`

Read the [usage guide](../stdlib/string.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class StringIterator : Iterator<char>`

Cursor produced by `string.iterator()`.

```dream
public class StringIterator : Iterator<char>
```

## `iterator`

So `for (let c in s.chars())` uses the enumerator protocol on the cursor itself.

```dream
public fun iterator(): StringIterator
```

## `next`

Yields the next UTF-16 code unit or None.

```dream
public fun next(): Option<char>
```
