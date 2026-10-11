# String search, regex, async scheduler and native header (2026-10-11, Windows x64)

Follow-up to `runtime-perf-2026-10-09.md`. Native, current against C# on the same runner.

| macro | before | after |
|---|---|---|
| log_processor | 1.33 | 0.98 (inconclusive) |
| async_fanout | 1.18 | 0.56 |
| task_parallel | 1.16 | 1.42 (CI 1.18-1.60; unchanged code path, run-to-run spread) |

- **String search.** `string_view_find_char` scanned one UTF-16 unit at a time, and
  `split(char)` called it once per separator just to size its result. The first eight units are
  still scanned inline (split pieces are usually that short); longer runs go to an out-of-line
  eight-unit vector compare, which lowers to SSE2/NEON/simd128 under the program's CPU.
  `split(char)` sizes its result with a new vector `string_view_count_char`. Line splitting in
  `log_processor` 80 -> 30 µs, word splitting 160 -> 137 µs. `split_span` costs 18.7 ns against
  16.9 for the pure scalar search (pieces of 4-7 units); still 0.57× C#.
- **Regex validation.** Patterns compiled with `PCRE2_UTF` made every match re-validate the
  whole subject. `PCRE2_MATCH_INVALID_UTF` lets the JIT skip that, and a lone surrogate now
  just fails to match instead of failing the whole call. `log_processor` regex stage 154 -> 110 µs.
- **Futures.** Every future came from the locked shared heap and freed back through it, and
  every `dream_await` took the foreign-completion lock. Futures are now private allocations
  (`dream_complete_foreign`, the only cross-thread path, already publishes the frame) and only
  host-kind children take the lock. 64×4 fan-out with `Promise.all`: 48.6 -> 22.4 µs; micro
  `async_poll` 82 -> 31.5 ns, `scheduler_queue` 306 -> 107 ns.

## Native header 32 -> 16 bytes

The native block header was `[size][type info][padding][magic][tag][count]`. It is now
`[type info | size class << 56][tag][count]` (32-bit hosts keep the class in its own word), the
same 16 bytes as wasm. The size comes from the class; blocks over 64 KiB carry a 16-byte prefix
holding their size. A free block keeps its free-list link in the first word and a freed tag
(`0x7fffffff`), which sends a second free to the slow path where it is ignored. The magic word
is gone, so release builds no longer detect double frees beyond that tag check.

A/B against the previous commit (its own runtime, built in a worktree), 10 rounds × 3 passes:

| micro | old ns | new ns | new/old |
|---|---|---|---|
| alloc_churn | 5.57 | 3.87 | 0.69 |
| binary_trees_alloc | 30,860 | 22,618 | 0.73 |
| linked_walk | 1,088 | 947 | 0.86 |
| arc_locals | 6.02 | 5.65 | 0.94 |
| json_deserialize | 452 | 427 | 0.95 |
| binary_trees | 23,992 | 24,224 | 1.01 |
| string_concat | 4.09 | 4.28 | 1.06 (inconclusive) |

`linked_walk` now matches C# (0.98-1.00): a 1024-node chain is 32 KB instead of 48 KB and stays
in L1. Region allocation (`binary_trees` runs inside an inferred region) first regressed 11%:
deriving the class there cost three cycles per object until the common <= 256-byte case got
its own branch ahead of the large-block test. Peak RSS on the micro suite 88.2 -> 81.5 MB.

## Weak fields, bounds checks, string capacity, Task fan-out

Against `cb1e219c` (its own runtime, built in a worktree) and C#:

| bench | old | new | new/old | new/C# |
|---|---|---|---|---|
| weak_tree | 64 µs | 36 µs | 0.55 | 0.90 |
| scratch_arena | 0.82 ns | 0.58 ns | 0.71 | 1.02 |
| string_concat | | | 0.95 | |
| string_builder | | | 0.92 | |
| task_parallel | ~160 µs (±20%) | 87.7 µs (±0.3%) | 0.56 | 1.11 |

- **Weak fields.** Constructing `Some(x)` into a niche weak field retained `x` only for the weak
  store to release it; the payload is now stored borrowed. Only the transition to a zero count
  takes the weak lock (loaders only revive a nonzero count), and dropping a holder peeks its
  slots unlocked, so empty slots cost nothing.
- **Bounds checks.** `dream_array_at` takes an `i32` index and checks it with one unsigned
  compare against a length known non-negative, so LLVM drops the check for `start / 2`-style
  indices and turns the division into a shift.
- **String capacity.** In-place appends asked for the block's capacity in units; they now ask
  whether the block's size class fits the needed bytes.
- **Task fan-out.** On Windows the CRT `getenv` locks and compares case-insensitively; the
  `seed_bit` lookup alone cost 2.4 µs per call. `processEnvGet` now calls
  `GetEnvironmentVariableW`. A session holds a pool thread only while it has queued work, so
  idle `Task`s no longer pin threads. Fan-out overhead per 16 tasks went from 45 to 17 µs.

`substring_span` stays at C# parity (0.62 vs 0.60 ns). `json_service` had been about 4% slower
since the header change, all of it in deserialization. Growing a buffer through `Buffer.realloc`
costs about 1 ns more per step than before, for no reason visible in the source, and every
JSON string went through a 16-byte `StringBuilder` that grew several times. `parse_string` now
sizes the builder to the run before the first quote or backslash, so escape-free strings never
grow: `json_service` 1.00× the old runtime (0.12× C#).

A synchronous `Task` body that started a future with `Promise.start` leaked it natively: only
async bodies ran the scheduler. Every job now drains leftover ready or timed work before its
reply, as `main` does before exiting. On wasm32 a worker whose job left a host call unsettled is
terminated instead of pooled.

### wasm32

The wasm macro suite trapped in `task_parallel` with "allocation size exceeds the WASI heap
limit", both before and after these changes. Every `Task` started a JS worker with a new
instance, and each instance claimed a 2 MiB private slab that nothing could reach once the
instance was gone. Idle workers are now pooled (up to 64). A reused worker hands its private
slab tail and free lists back to the shared heap (`dream_heap_thread_exit`), then restarts its
stack and thread-locals in place, so each session still starts as a fresh thread. A slab refill
adopts any free block of at least 1 MiB before claiming new memory. 600 sequential tasks now
keep the heap pointer flat; it used to grow 2 MiB per task.

| macro (wasm, current) | ns/op | vs old runtime |
|---|---|---|
| async_fanout | 29,397 | 0.67 |
| json_service | 4,706 | 0.99 |
| log_processor | 954,825 | 0.97 |
| graph_paths | 779,208 | 1.03 |
| task_parallel | 256,499 | ~0.95 (old completes only with one pass per process) |
