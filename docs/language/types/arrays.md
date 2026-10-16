---
title: Arrays
summary: "Fixed-size, ordered values of one type."
icon: material/code-array
status: stable
targets: [native, web, node]
---

# Arrays

An array holds a fixed number of values of the same type, in order. Passing an array shares its contents rather than copying them. Use a [List](../../library/collections/list.md) when you need to add or remove items.

!!! glance "At a glance"

    ```dream
    let nums = [1, 2, 3];
    let first = nums[0];
    nums[2] = 99;
    ```

    - An array has a fixed length and holds values of one type.
    - Passing an array shares its contents; use `List<T>` when it needs to grow.

## Creating, reading, writing

List the values inside `[...]`; all elements must share a type. Access is zero-indexed:

```dream
let nums = [1, 2, 3, 4, 5]; // int[]
let words = ["red", "green", "blue"]; // string[]

let first = nums[0]; // 1
nums[2] = 99; // [1, 2, 99, 4, 5]
```

!!! note
    Indexing out of bounds — including with a negative index — [panics](../rules/panics.md): the program prints a message and halts. It is not undefined behavior, but it is fatal and non-recoverable, so keep indices in range rather than relying on the check.

## Size

`.length` returns the element count. `List`, `Map`, `Set`, and every other `Collection` expose the same `length` property, so every collection is measured the same way:

```dream
let count = nums.length; // 5
```

## Passing to functions

Because arrays are references, a function sees the caller's buffer directly:

```dream
fun fill_zeros(arr: int[]): void {
    let i = 0;
    while i < arr.length {
        arr[i] = 0;
        i = i + 1;
    }
}
```

## Arrays of classes and nested arrays

The element type can be a class, or another array for multi-dimensional data:

```dream
class Point {
    x: int;
    y: int;
}

let pts: Point[] = [Point(0, 0), Point(1, 2)];
System.println(pts[1].x); // 1

let grid: int[][] = [[1, 2, 3], [4, 5, 6]];
System.println(grid.length); // 2  (rows)
System.println(grid[0].length); // 3  (columns)
System.println(grid[1][2]); // 6
```

## Repeat arrays: `[value; len]`

`[value; len]` allocates a `T[]` of length `len` whose every slot holds `value`. The element type is inferred from the value (or from the surrounding array-typed context), and `len` may be any runtime `int` expression:

```dream
let zeros = [0; 4]; // int[4], all zero
let sevens = [7; 3]; // [7, 7, 7]
let n = read_count();
let buf = [2.0; n * 2]; // length computed at runtime
```

The value expression is evaluated **once**, and every slot shares the result — for reference types (`["hi"; 3]`, `[config; 8]`) all slots point at the same object. The one exception is a value that itself constructs an array: then it is re-evaluated per slot so each row is distinct:

```dream
let grid = [[0; 3]; 5]; // int[][] — five *distinct* rows of three zeros
grid[0][1] = 42;
System.println(grid[1][1]); // still 0
```

A zero value (`0`, `0.0`, `false`) fills the array with zeros.

## Fixed-size buffers

Array literals produce a fixed-size `T[]`; you cannot push or pop. For an explicitly zero-initialized buffer of a runtime length, use `Array.alloc<T>(n)` (no extra import needed). `Buffer.alloc<T>(n)` is the another way to request the same array:

```dream
let buf = Array.alloc<int>(4); // int[] of length 4, all zero
buf[0] = 10;
```

There is no `new int[n]` / `int[5][10]` syntax — use `[value; len]` for a filled buffer, or `Array.alloc<int[]>(rows)` followed by per-row allocation when rows must be built individually (inner slots are null until you assign them):

```dream
let grid = Array.alloc<int[]>(5);
let r = 0;
while r < 5 {
    grid[r] = Array.alloc<int>(10);
    r = r + 1;
}
grid[0][0] = 32;
```

`Buffer.realloc<T>(arr, new_len)` and `Buffer.free<T>(arr)` (both [`@unsafe`](../memory/buffers.md#unsafe-manual-memory-management)) manage an array's backing block directly through the allocator instead of through ARC: `realloc` resizes it in place (preserving the overlapping prefix, zero-filling any grown tail) and `free` returns it immediately, bypassing reference counting. `arr` must have exactly one owner going into either call — the old value must never be read again afterward. Use ordinary arrays for managed elements; [`RawPtr<T>`](arrays.md#rawptrt-raw-memory-unsafe) provides separate raw storage for unmanaged elements.

!!! note "Arrays own their slots"
    Every slot of a `T[]` is released when the array is dropped, even if your own bookkeeping
    stopped tracking it earlier — see [Raw buffers and custom containers](../memory/buffers.md#raw-buffers-and-custom-containers).
    Overwriting a slot releases its previous occupant immediately.

## `Span<T>`: a bounds-checked view without copying

`Span<T>` is a [`ref struct`](classes-structs.md#ref-struct-a-stack-only-value-type) that views a contiguous run of an existing array's elements — no copy, no heap allocation of its own, and its own logical range is enforced independently of the backing array's actual size. Because it is a `ref struct`, the compiler rejects any use that would let one escape the stack frame that created it (a field, a generic type argument, a lambda capture, or an `async` parameter):

```dream
let xs = [1, 2, 3, 4, 5];
let whole = Span.of(xs); // inferred Span<int> — a span over all of xs
let mid = whole.slice(1, 3); // [2, 3, 4] — still a view, no copy

System.println(mid.get(0)); // 2
mid.set(0, 20); // writes through to xs[1]
System.println(xs[1]); // 20

let owned = mid.to_array(); // copies into a fresh, independently-owned array
```

`Span<T>` keeps its backing array strongly referenced (unlike `RawPtr<T>` below), so the memory it views can never be freed out from under it. Prefer `Span<T>` over a raw index range whenever a function only needs to read/write a *slice* of an array without owning or resizing it.

`ReadOnlySpan<T>` is the same view without writes (`span.as_read_only()`, or `ReadOnlySpan.of(xs)`), and `list.as_span()` / `list.as_read_only_span()` view a `List<T>`'s live elements. `for (let x in span)` iterates any of them without allocating an iterator. See [Spans](spans.md) for the full API, the string view `ReadOnlySpan<char>`, and what makes spans free at runtime.

## `RawPtr<T>`: raw memory (`@unsafe`)

`RawPtr<T: unmanaged>` carries an address without a length, array header, or ARC ownership.
Allocation is uninitialized by default. Reads, writes, indexing, casts and pointer arithmetic
require `@unsafe` and do not check bounds. Initialize a value before reading it.

```dream
@unsafe
fun scratch(): void {
    let p = RawPtr<int>.alloc(4);
    p.write(10);
    p[1] = 20;
    System.println(p.offset(1).read());
    p = p.realloc(8);
    p[7] = 70;
    p.free();
}
```

Use `alloc_zeroed` when zeroed bytes are required, and `alloc_aligned` for an explicit
power-of-two alignment at least `alignof(T)`. Reallocation preserves the overlapping prefix
and leaves the new tail uninitialized. It invalidates every old alias. Only base pointers returned by RawPtr allocation methods may be freed or resized;
foreign pointers require their originating deallocator.
`copy_to` requires non-overlapping ranges; `move_to` permits overlap. Both count elements.
`fill_bytes` counts bytes. Copies never keep storage alive.

`Arena(initial_capacity)` owns manually freed blocks, measured in bytes. Its generic
`alloc<T>(count)` methods support different unmanaged types in one arena. Growth adds blocks
without moving existing allocations. Reset invalidates every allocation but keeps the blocks
for reuse; free releases all blocks. Arena allocations cannot be individually freed or resized.

```dream
@unsafe
fun batch(): void {
    let arena = Arena(4096);
    let numbers = arena.alloc<int>(100);
    let bytes = arena.alloc_zeroed<byte>(256);
    numbers[0] = 42;
    bytes[0] = (byte)7;
    arena.reset();
    arena.free();
}
```

Both types are value handles with manual lifetime contracts, and neither has automatic cleanup.
Copying an arena aliases the same state; free that state once. Strings, arrays, classes and
values containing managed references cannot be raw elements. Use ordinary arrays and spans
when automatic ownership and bounds checking meet your needs.

## Advanced: growable arrays

### `List<T>`

[`List<T>`](../../library/collections/index.md) (**package:** `system.collections` — `import system.collections;`) is a class wrapping a `T[]` buffer that doubles on demand:

```dream
import system;
import system.collections;

let xs = List<int>();
xs.push(10);
xs.push(20);
System.println(xs.length); // 2
System.println(xs.get(0)); // 10
```

`List<T>` offers `push`, `pop`, `@get_indexer` / `@set_indexer` (so `xs[i]` / `xs.get(i)` return `T` and panic if out of range; `xs[i] = v` writes through), `contains`, `index_of`, `remove_at`, `clear`, and `@iterator` (so `for (let x in xs)` works). Nested lists support `list[i][j]`. When the element type is `Comparable`, `sort()` and `binary_search()` are also available:

```dream
let ys = List<int>();
ys.push(3);
ys.push(1);
ys.push(2);
ys.sort(); // 1, 2, 3
System.println(ys.binary_search(2).unwrap_or(-1)); // 1
```

### Collection protocols

`Iterator<T>`, `Collection<T>`, and `IndexedCollection<T>` live in bootstrap `system.core`.

- `Collection<T>` — `length` and `iterator()` (plus default `is_empty()` and query helpers like `all` / `any`). Implemented by `List`, `Set`, `Map`, `Queue`, `Stack`, and every `T[]`.
- `IndexedCollection<T>` — extends `Collection<T>` with ordered indexable access (`get(index)`, defaults `first` / `last`). Implemented by `List` and by arrays (`extend T[]` in bootstrap).
- `for (let x in xs)` works for arrays (native index loop), concrete `@iterator` types, and interface-typed `Collection` / `IndexedCollection` / `Iterator`.

```dream
fun total_size(xs: Collection<string>): int {
    return xs.length;
}

fun sum(xs: Collection<int>): int {
    let n = 0;
    for (let x in xs) {
        n = n + x;
    }
    return n;
}

fun from_array(xs: IndexedCollection<int>): int {
    return xs.first().unwrap_or(0);
}

fun main(): void {
    let arr: int[] = [1, 2, 3];
    System.println(sum(arr)); // arrays upcast to Collection
    System.println(from_array(arr));
}
```
