# Choose when cleanup runs

Use deferred cleanup when destroying a value during a sensitive part of a program would cause an unwanted pause.

[Back to overview](memory.md)

## `defer`: wait until after the important work to run destructors

Normally, when nothing points at an object anymore, Dream runs its `del` (if any) and frees it **right then**.
That is what you want almost everywhere.

Sometimes that “right then” is a bad moment: you drop last year’s UI tree or a particle buffer, and the destructor storm runs **before** you finish drawing or simulating this frame.
`defer { … }` keeps the objects logically gone (nothing can use them), but **runs the actual cleanup at `}`** — after the work you care about.

```dream
defer {
    old_root = new_tree();   // old tree is no longer needed
    paint(new_tree);         // do this before a huge cleanup hitch
} // `del` / free of the old tree runs here
```

**Use it** when there is a deadline in the middle of a tick (paint, simulate, submit a frame) and a large graph dies in the same tick.

**Skip it** when you are just allocating and dropping in a loop with nothing urgent in between.
Cleanup is not cheaper with `defer` — it is only **later** (and can use a bit more memory until `}`).
Needless `defer` is extra bookkeeping.

Braces are required.
`await` is not allowed inside `defer`.
GPU shaders do not support it.

```dream
class Tracked {
    public id: int;
    del() { System.println(this.id); }
}

fun main() {
    defer {
        let x = Tracked(1);
        System.println(x.id);  // last use of `x` — `del` waits
        System.println(999);
    } // now prints 1
}
```

### Optional: how much to clean at `}`

- `defer { … }` — clean a batch at `}` (256 objects), then **finish the rest** if this is the outermost `defer`, so work does not sit until the program exits.
- `defer(q) { … }` — `q` is a `uint` (plain `256` is fine). That many objects are cleaned at this `}`. `defer(0)` means “don’t clean on this `}`” — useful around a game loop so inner `defer(256)` slices can spread cleanup across frames.
- Nested `defer` share one cleanup list. While you are still inside some `defer`, leftover work can wait for the next one, but the list is capped (16 384 objects) so memory cannot grow without bound.
- When you compile for the browser or Node, memory is freed as soon as the last use ends. `dream run` may wait until the end of the block.
- On native, a last-ref **string** that the compiler releases is queued like other last-refs (the free waits for `}`). Helpers such as in-place concat still free string temps immediately.

A timing sample (UI tree swap + particles): `dream --release run sample/defer_destroy_bench.dream`.
