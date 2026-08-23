# Arrays

An array is a fixed-size, ordered block of same-typed values. Arrays are reference types, so passing one around shares the same buffer rather than copying it. For a growable sequence, reach for [`List<T>`](../stdlib/collections.md).

## Creating, reading, writing

List the values inside `[...]`; all elements must share a type. Access is zero-indexed:

```dream
let nums = [1, 2, 3, 4, 5];            // int[]
let words = ["red", "green", "blue"];  // string[]

let first = nums[0];   // 1
nums[2] = 99;          // [1, 2, 99, 4, 5]
```

!!! note
    Indexing out of bounds — including with a negative index — [panics](panics.md): the program prints a message and halts. It is not undefined behavior, but it is fatal and non-recoverable, so keep indices in range rather than relying on the check.

## Size

`.length` returns the element count. It is the same `size()` that `List` and `Map` expose, so every collection is measured the same way:

```dream
let count = nums.length;   // 5
```

## Passing to functions

Because arrays are references, a function sees the caller's buffer directly:

```dream
fun fill_zeros(arr: int[]): void {
    let i = 0;
    while (i < arr.length) {
        arr[i] = 0;
        i = i + 1;
    }
}
```

## Arrays of classes and nested arrays

The element type can be a class, or another array for multi-dimensional data:

```dream
class Point { x: int; y: int; }

let pts: Point[] = [ Point(0, 0), Point(1, 2) ];
println(pts[1].x);   // 1

let grid: int[][] = [[1, 2, 3], [4, 5, 6]];
println(grid.length);      // 2  (rows)
println(grid[0].length);   // 3  (columns)
println(grid[1][2]);       // 6
```

## Repeat arrays: `[value; len]`

`[value; len]` allocates a `T[]` of length `len` whose every slot holds `value`. The element type is inferred from the value (or from the surrounding array-typed context), and `len` may be any runtime `int` expression:

```dream
let zeros = [0; 4];        // int[4], all zero
let sevens = [7; 3];       // [7, 7, 7]
let n = read_count();
let buf = [2.0; n * 2];    // length computed at runtime
```

The value expression is evaluated **once**, and every slot shares the result — for reference types (`["hi"; 3]`, `[config; 8]`) all slots point at the same object. The one exception is a value that itself constructs an array: then it is re-evaluated per slot so each row is distinct:

```dream
let grid = [[0; 3]; 5];   // int[][] — five *distinct* rows of three zeros
grid[0][1] = 42;
println(grid[1][1]);      // still 0
```

A zero-like value (`0`, `0.0`, `false`) on a scalar element type skips the fill loop entirely and lowers straight to a zero-initialized allocation.

## Fixed-size buffers

Array literals produce a fixed-size `T[]`; you cannot push or pop. For an explicitly zero-initialized buffer of a runtime length, use `Array.alloc<T>(n)` (bootstrap; no extra import). `Buffer.alloc<T>(n)` is the same intrinsic underneath:

```dream
let buf = Array.alloc<int>(4);   // int[] of length 4, all zero
buf[0] = 10;
```

There is no `new int[n]` / `int[5][10]` syntax — use `[value; len]` for a filled buffer, or `Array.alloc<int[]>(rows)` followed by per-row allocation when rows must be built individually (inner slots are null until you assign them):

```dream
let grid = Array.alloc<int[]>(5);
let r = 0;
while (r < 5) {
    grid[r] = Array.alloc<int>(10);
    r = r + 1;
}
grid[0][0] = 32;
```

`Buffer.realloc<T>(arr, new_len)` and `Buffer.free<T>(arr)` (both [`@unsafe`](memory.md#unsafe-manual-memory-management)) manage an array's backing block directly through the allocator instead of through ARC: `realloc` resizes it in place (preserving the overlapping prefix, zero-filling any grown tail) and `free` returns it immediately, bypassing reference counting. `arr` must have exactly one owner going into either call — the old value must never be read again afterward. Most code should reach for [`Pointer<T>`](arrays.md#pointert-manual-allocation-unsafe) instead of calling these directly.

!!! note "Arrays own their slots"
    Every slot of a `T[]` is released when the array is dropped, even if your own bookkeeping
    stopped tracking it earlier — see [Raw buffers and custom containers](memory.md#raw-buffers-and-custom-containers).
    Overwriting a slot releases its previous occupant immediately.

## `Span<T>`: a bounds-checked view without copying

`Span<T>` is a [`ref struct`](classes-structs.md#ref-struct-a-stack-only-value-type) that views a contiguous run of an existing array's elements — no copy, no heap allocation of its own, and its own logical range is enforced independently of the backing array's actual size. Because it is a `ref struct`, the compiler rejects any use that would let one escape the stack frame that created it (a field, a generic type argument, a lambda capture, or an `async` parameter):

```dream
let xs = [1, 2, 3, 4, 5];
let whole = Span.of(xs);           // inferred Span<int> — a span over all of xs
let mid = whole.slice(1, 3);       // [2, 3, 4] — still a view, no copy

println(mid.get(0));               // 2
mid.set(0, 20);                    // writes through to xs[1]
println(xs[1]);                    // 20

let owned = mid.to_array();        // copies into a fresh, independently-owned array
```

`Span<T>` keeps its backing array strongly referenced (unlike `Pointer<T>` below), so the memory it views can never be freed out from under it. Prefer `Span<T>` over a raw index range whenever a function only needs to read/write a *slice* of an array without owning or resizing it.

## `Pointer<T>`: manual allocation (`@unsafe`)

`Pointer<T>` is a manually-managed handle to a `T[]` block, allocated, resized, and released through the allocator directly (`Buffer.alloc`/`Buffer.realloc`/`Buffer.free`) rather than through [automatic reference counting](memory.md). Every operation that touches the block's lifetime is [`@unsafe`](memory.md#unsafe-manual-memory-management): the compiler cannot verify the block has exactly one owner, that `free()` runs at most once, or that no access happens after a `free()`.

```dream
@unsafe
fun scratch(): void {
    let p = Pointer<int>.alloc(4);   // zero-initialized, 4 elements
    p.set(0, 10);
    println(p.get(0));               // 10

    p.realloc(8);                    // grow in place; [0..4) preserved, [4..8) zeroed
    println(p.length);                // 8

    p.free();                        // returns the block to the allocator immediately
}
```

Prefer `Span<T>` unless a value specifically needs to outlive the callee's stack frame, or the workload needs C-style manual alloc/realloc/free (e.g. a long-lived off-heap buffer). See [Memory Management](memory.md) for the full `@unsafe` contract.

## Advanced: growable arrays

### `List<T>`

[`List<T>`](../stdlib/collections.md) (**package:** `system.collections` — `import system.collections;`) is a class wrapping a `T[]` buffer that doubles on demand:

```dream
import system;
import system.collections;

let xs = List<int>();
xs.push(10);
xs.push(20);
System.println(xs.length);                // 2
System.println(xs.get(0));  // 10
```

`List<T>` offers `push`, `pop`, `@get_indexer`/`@set_indexer` (so `xs[i]` / `xs.get(i)` return `T` and panic if out of range; `xs[i] = v` writes through), `contains`, `index_of`, `remove_at`, `clear`, and `@iterator` (so `for (let x in xs)` works). Nested lists support `list[i][j]`. When the element type is `Comparable`, `sort()` and `binary_search()` are also available:

```dream
let ys = List<int>();
ys.push(3); ys.push(1); ys.push(2);
ys.sort();                                   // 1, 2, 3
System.println(ys.binary_search(2).unwrap_or(-1));  // 1
```

### Collection protocols

`Iterator<T>`, `Collection<T>`, and `IndexedCollection<T>` live in bootstrap `system.core`.

- `Collection<T>` — `size()` and `iterator()` (plus default `is_empty()` and query helpers like `all` / `any`). Implemented by `List`, `Set`, `Map`, `Queue`, `Stack`, and every `T[]`.
- `IndexedCollection<T>` — extends `Collection<T>` with ordered indexable access (`get(index)`, defaults `first`/`last`). Implemented by `List` and by arrays (`extend T[]` in bootstrap).
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
    System.println(sum(arr));       // arrays upcast to Collection
    System.println(from_array(arr));
}
```
