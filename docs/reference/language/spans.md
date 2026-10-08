# Spans

A span is a view of part of a string, array or list. It stores no copy of the data: just the source, an offset and a length. Spans are [`ref struct`](classes-structs.md#ref-struct-a-stack-only-value-type)s, so they live on the stack. Making, slicing and comparing one never allocates.

```dream
import system;

fun main() {
    let line = "name=Ada; lang=Dream";
    let key = line.span(0, 4);              // "name", no allocation
    System.println(key == "name");     // true
    System.println(key.length);             // 4
    let owned = key.to_string();            // the explicit allocation point
    System.println(owned);
}
```

| View | Over | Get one with |
| --- | --- | --- |
| `StringSpan` | `string` text | `s.span()`, `s.span(start, end)`, `builder.as_span()` |
| `Span<T>` | `T[]` elements, writable | `Span.of(xs)`, `Span(xs, offset, length)`, `list.as_span()` |
| `ReadOnlySpan<T>` | `T[]` elements, read-only | `ReadOnlySpan.of(xs)`, `span.as_read_only()`, `list.as_read_only_span()` |

## `StringSpan`

`Span<char>` permits writes to a `char[]`; strings are immutable and may use sliced storage.
`StringSpan` keeps the string owner alive and views its UTF-16 units directly, without converting
them into an array. Both views are stored inline. Release can hoist the string payload address
out of read-only loops when the source lifetime and view fields are proven stable.

`s.span(start, end)` clamps its bounds exactly like `s.substring(start, end)`, but returns a view instead of a new `string`. It supports the read-only `string` surface:

| Call | Meaning |
| --- | --- |
| `length` / `is_empty()` / `byte_size()` | size in UTF-16 units / payload bytes |
| `char_at(i)` / `sp[i]` / `byte_at(i)` | one code unit / payload byte |
| `==` / `equals(s)` / `compare` | equality with a `StringSpan` or `string` / named string equality / ordering against either |
| `starts_with` / `ends_with` / `contains` | tests |
| `index_of` / `last_index_of` | `Option<int>`, by `char` or `string` |
| `slice(start, end)` / `slice(start)` | a narrower view, clamped |
| `trim` / `trim_start` / `trim_end` | a view without surrounding whitespace |
| `parse_int()` / `parse_double()` | `Result` like the `string` versions |
| `split_iter(sep)` / `lines()` | allocation-free iterators of `StringSpan` pieces |
| `to_string()` | copy the view into an owned `string` |

`==` and `!=` compare contents without allocating: `sp == "text"`, `"text" == sp`,
and `sp == other_span` all work. `equals(string)` is also available.

A span hashes like the equal `string`, so `Map<string, V>` and `Set<string>` look it up without building a key:

```dream
let ages = Map<string, int>();
ages.set("ada", 36);
let text = "ada,grace";
System.println(ages.get_or(text.span(0, 3), 0));   // 36
```

`Map.get`, `Map.get_or`, `Map.contains` and `Set.contains` accept a `StringSpan` when the key type is `string`.

Split and line iterators are cursors: call `move_next()`, then read `current`.

```dream
let it = "a,b,,c".span().split_iter(',');
while it.move_next() {
    System.println(it.current.length);   // 1, 1, 0, 1
}
```

`StringBuilder.as_span()` views the text built so far. The view stays valid after more appends or a `clear()`: the builder moves to a new buffer instead of overwriting text a span still sees.

## `Span<T>` and `ReadOnlySpan<T>`

Both view `array[offset .. offset + length)`. Creating one with a range outside the array panics, so every later access only checks the span's own length.

| Call | `Span<T>` | `ReadOnlySpan<T>` |
| --- | --- | --- |
| `length` / `is_empty()` | yes | yes |
| `sp[i]` / `get(i)` | yes | yes |
| `sp[i] = v` / `set(i, v)` / `fill(v)` / `copy_from(src)` | yes | no |
| `slice(start, count)` / `slice(start)` | yes | yes |
| `index_of(v)` / `contains(v)` / `sequence_equal(other)` | yes | yes |
| `copy_to(dst)` / `to_array()` | yes | yes |
| `as_read_only()` | yes | — |

`for (let x in span)` walks either kind with an index loop; no iterator object is allocated. Copies and comparisons of `unmanaged` element types use bulk memory operations.

`list.as_span()` views the list's elements at the moment it is called. Like C#'s `CollectionsMarshal.AsSpan`, it keeps the backing array it saw alive: if the list later grows into a new array, the span still shows the old one.

## Where a span can go

A span may be a local, a parameter, a return value, or a field of another `ref struct`. The compiler rejects anything that could let it outlive its stack frame:

- a field of a `class` or ordinary `struct`
- a generic type or function argument (`List<StringSpan>`, `identity(sp)`)
- an array element
- a lambda capture
- an `async` parameter, or a local still in scope at an `await`

The full list, with the reasoning, is under [`ref struct`](classes-structs.md#ref-struct-a-stack-only-value-type).

## Cost

A span holds its source strongly, so the data can never be freed while you use it. The compiler removes that cost where it can prove it is unnecessary:

- A span built from a `borrow` parameter, a string literal, or another such span does no reference counting. Its fields stay in registers, so `span.length` in a loop often folds to a constant.
- Span indexers check bounds against the span's own length. In `while i < sp.length` loops the optimizer removes the check, and simple loops vectorize.
- Where a `substring` result replaces a previous one in a loop, the old slice block is reused in place.

Prefer spans over `substring` for parsing and lookups. Call `to_string()` only for values that must outlive the frame, such as map keys you insert or fields you store.
