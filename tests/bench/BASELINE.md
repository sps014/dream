# Microbench baseline and C# parity notes

## Measurement protocol (read before comparing numbers)

`./scripts/run-microbenches.sh` runs the whole suite `REPS` times (default 5; override with
`REPS=7`) wrapped in `caffeinate`, then prints per-bench **median** and spread%
(`(max-min)/median`, flagged `!` above 15%). The script does not print mins; take them from
the per-rep files `out/native.repN.txt` / `out/csharp.repN.txt`. Rules for honest numbers:

- **Compare `min` columns** — the minimum converges to true cost even on a loaded machine;
  the median tracks ambient load. Two back-to-back runs on this host agreed within 0-2% on
  compute kernels (`json_*`, `iface_dispatch`) and ~10-15% elsewhere while load average was
  6-7 (opencode/VS Code/Chrome running). On an idle machine (load < 1) expect 1-3%.
- Rows flagged `!` are load-sensitive; re-run before trusting a regression there.
- ns_per_op values are fractional now (integer division used to truncate sub-ns benches to
  0-1); the comparator recomputes from `ns_total/iters`.

Recorded with `./scripts/run-microbenches.sh` (Dream `--release` native C, optional
`dotnet run -c Release` from `tests/bench/csharp`). Absolute values vary by host — use
relative deltas. Dream and C# are **different substrates** (Wasm+ARC vs native JIT+GC);
ratios are not an ARC-only scoreboard.

## API fairness notes

| Bench | Dream | C# |
|-------|-------|-----|
| `char_scan` | `char_at` loop over UTF-16 code units | `foreach` over UTF-16 chars |
| `byte_scan` | `byte_at` walk of UTF-16 LE payload (`byte_size` = 2 × `length`) | same payload, `MemoryMarshal.AsBytes` (two bytes per code unit) |
| `substring` | `substring(start, end)` | `Substring(start, length)` |
| `scratch_arena` | `bump` / `set_at` / `at` (no Span RC) | same index API |
| `regex_find` | Global `[a-z]+\d+` via Pike VM (not bare `\d+`) | same pattern, source-generated regex |
| `json_serialize` / `json_deserialize` | Nested `@json` User+Address, payload built once; deserialize text outside timer; scale `/10` | `System.Text.Json` source generation |
| `arr_add` | Scalar `c[i]=a[i]+b[i]` (`float[]`+`int[]`, n=256); Dream autovecs to `v128` | same scalar `for` (RyuJIT autovec) |
| `vec_add` | `Vector<float>` stride + scalar tail (`count()` lanes; WASM `v128` locals) | `System.Numerics.Vector<float>` |

## Campaign snapshots (Dream ns/op)

### Before (campaign start)

| Bench | ns_per_op (approx) |
|-------|-------------------:|
| substring | 2547 |
| list_insert_mid | 543 |
| map_clear_reuse | 106 |
| string_builder | 405 |

### After (ARC + stdlib opts)

| Bench | ns_per_op (approx) | vs before |
|-------|-------------------:|----------:|
| substring | ~180 | ~14× faster |
| list_insert_mid | ~170 | ~3× faster |
| map_clear_reuse | ~65 | ~1.6× faster |

### After (clear reuse / SROA / ScratchArena / regex SOA)

| Bench | ns_per_op (approx) | notes |
|-------|-------------------:|-------|
| list_clear_reuse | ~9 | in-place clear, capacity kept |
| map_clear_reuse | ~46 | states/slots zeroed in place |
| scratch_arena | ~41 | bump + Span |
| regex_find | ~33k | SOA queues + reused mark/caps buffers |
| list_insert_mid | ~96 | |
| substring | ~127 | |

### After (parity campaign — honest regex_find)

Do **not** treat the earlier “regex ~767 vs C# ~700” row as Pike-VM parity: that used bare `\d+`
and hit a digit-run fast path. Headline `regex_find` is now `[a-z]+\d+` (real VM).

### Wasm hotpath (list/map clear+insert, concat+itoa)

Representative Dream vs C# after those opts (same host):

| Bench | Dream | C# |
|-------|------:|---:|
| alloc_churn | 21 | 23 |
| list_clear_reuse | 5 | 1 |
| scratch_arena | 5 | 1 |
| string_eq | 7 | 5 |
| list_push | 6 | 1 |
| list_insert_mid | 37 | 15 |
| string_builder | 22 | 7 |
| map_clear_reuse | 33 | 3 |
| arc_locals | 44 | 21 |
| string_concat | 50 | 13 |
| map_get_set | 56 | 10 |
| substring | 119 | 6 |
| byte_scan / char_scan | 228 / 286 | 21 / 23 |
| regex_find | ~14k | ~820 |

### After (JSON benches, autovec, `Vector<T>`)

Same host as `./scripts/run-microbenches.sh`. `arr_add` is the autovec row (`f32x4.add` /
`i32x4.add` in `--release` WAT plus a scalar remainder). `vec_add` is explicit `Vector<T>`
and currently pays extra `v128` store/reload through struct sret (not a register SIMD loop
like RyuJIT).

| Bench | Dream | C# |
|-------|------:|---:|
| json_serialize | 1220 | 961 |
| json_deserialize | 10121 | 1753 |
| arr_add | 449 | 369 |
| vec_add | 3375 | 79 |
| regex_find | 13139 | 835 |
| string_builder | 22 | 12 |
| map_clear_reuse | 24 | 2 |
| string_concat | 41 | 12 |

### After (`Vector` `v128` locals, typed JSON parse, Pike skip)

Same host as `./scripts/run-microbenches.sh`. Owning `Vector<T>` is a WASM `v128` local
(`v128.load` / lane op / `v128.store`; inlined `this` included). `json_deserialize<T>`
fills `T` from `JsonParser` (`from_json_parser_text`); `Json.deserialize<JsonValue>` / `from_json` stay for
the dynamic tree. `regex_find` is still Pike `[a-z]+\d+` (ASCII byte skip when the hint is
kind 5/7). `"hello" + i.to_string() + "world"` is `$concat_str_int_str`.

| Bench | Dream | C# |
|-------|------:|---:|
| json_serialize | 1227 | 986 |
| json_deserialize | 5327 | 1726 |
| arr_add | 479 | 339 |
| vec_add | 362 | 83 |
| regex_find | 12877 | 776 |
| string_builder | 24 | 7 |
| map_clear_reuse | 24 | 2 |
| string_concat | 40 | 9 |

### After (in-memory UTF-16 strings)

Heap `string` is UTF-16 LE code units (C#/JS `char` indexing). `.length` / `char_at` /
`substring` are O(1) `i32.load` / `i32.load16_u` / `memory.copy`. Same host as
`./scripts/run-microbenches.sh`.

| Bench | Dream | C# | vs prior Dream |
|-------|------:|---:|----------------|
| char_scan | 270 | 23 | ~286 → 270 (unit load vs UTF-8 decode) |
| byte_scan | 524 | 22 | walks UTF-16 LE payload (2× ASCII bytes) |
| substring | 29 | 6 | ~119 → 29 (no scalar-to-byte walk) |
| string_eq | 8 | 4 | ~7 |
| string_concat | 37 | 9 | ~40 |
| string_builder | 30 | 14 | ~24 (u16 buffer) |
| regex_find | 12606 | 956 | ~12877 (Pike on code units) |
| json_serialize | 1650 | 922 | ~1227 |
| json_deserialize | 5328 | 1726 | ~5327 |

### After (scan ABC + inlined loads, forwarding RC, map epoch clear)

`--release` WAT: `char_at`/`byte_at` are `i32.load16_u` / `i32.load8_u` with ABC dropping
the per-index `ge_u` on `while (i < s.length)` / `byte_size` loops (including interned
literals). Same-type forwarding copies (`let b = a`) are RC cursors. Unmanaged `Map`/`Set.clear`
bumps an occupancy epoch instead of `memory.fill`. `map_get_set` uses `get_or` (C# `TryGetValue`).
Same host as `./scripts/run-microbenches.sh`.

| Bench | Dream | C# | vs C# |
|-------|------:|---:|-------|
| char_scan | 115 | 24 | C# 4.8× |
| byte_scan | 216 | 27 | C# 8.0× (2× UTF-16 LE trips vs C# code units) |
| substring | 19 | 5 | C# 3.8× |
| string_concat | 30 | 9 | C# 3.3× |
| string_eq | 6 | 3 | C# 2.0× |
| arc_locals | 25 | 8 | C# 3.1× |
| list_push | 4 | 1 | C# 4.0× |
| list_insert_mid | 28 | 14 | C# 2.0× |
| list_clear_reuse | 5 | 1 | C# 5.0× |
| map_get_set | 46 | 8 | C# 5.8× (probe still a call; packed layout not landed) |
| map_clear_reuse | 25 | 2 | C# 12.5× (dominated by `set`, not clear) |
| scratch_arena | 4 | 1 | C# 4.0× |
| alloc_churn | 17 | 15 | ~parity |

### Fresh baseline (Aug 2026 — supersedes tables below)

> **Niche unions landed after this snapshot was taken**: `Option<ref>` is now the payload
> pointer itself (`None` = null). Effects vs the table below: **binary_trees 190k → ~60-70k
> ns/op (2.8-3×, at or better than C# 72k)**, json_deserialize 965→~900, regex_find 741→~700,
> string/alloc rows all slightly better; linked_walk unchanged (~4-5k, C# 1.7k still ahead on
> pointer chasing). The old headline finding "tracing GC beats ARC on tree churn" no longer
> holds natively.
>
> **ARC fast paths + chain-hop elision landed after that**: release glue split into an inline
> null-check + decrement at every Release site (call only on the free transition), plus a new
> `rc-hop-elision` pass that sinks the holder's release below a chain extract and cancels the
> borrow bracket on switch-arm bindings. Effects (quiet machine): **linked_walk ~4.5k → 1.5k
> ns/op — beats C# (1.7-2.0k)**; binary_trees ~57-70k; arc_locals 7→6; no regressions
> elsewhere. Per list hop: 3 RMWs + 2 calls → 1 RMW + 1 transition-only call.

Same host, `./scripts/run-microbenches.sh` — now a **three-way** table: Dream native C
(cc -O3; `-flto` is only passed on Linux — `host_cc_opt_flags` drops it on macOS and Windows,
so these macOS numbers are without LTO), Dream wasm32 under Node (`--wasm --release --runtime --node`), C# RyuJIT.
Also records `.wasm`/gz/br sizes at `-O3`/`-Os`/`-Oz` into `out/wasm_sizes.txt`.
New benches this round: `nbody`, `mandelbrot`, `matmul_64`, `quicksort`, `sieve`,
`fib_rec`, `iface_dispatch`, `binary_trees`, `linked_walk`, `wordcount`, `parse_ints`,
`sum_options` (+ `tco_sum` as an untimed TCO/stack sentinel in `main`).

| Bench | C | wasm | C# | note |
|-------|------:|------:|---:|------|
| nbody | 76 | 64 | 68 | ~parity |
| mandelbrot | 116k | 108k | 130k | Dream leads |
| matmul_64 | 238k | 215k | 235k | ~parity |
| quicksort | 25k | 27k | 52k | C 2.1× |
| sieve | 3.0k | 5.6k | 6.8k | C 2.2× |
| fib_rec | 32k | 44k | 38k | ~parity |
| iface_dispatch | 3.2 | 21 | 32 | devirt+inline wins; wasm dispatch cost visible (old bench shape — see Sep 24 2026) |
| binary_trees | 190k | 170k | 56k | **GC 3.4× faster** on tree alloc/free churn |
| linked_walk | 3.6k | 4.5k | 1.6k | **GC 2.3× faster** pointer chasing |
| wordcount | 21 | 22 | 24 | parity |
| substring | 1.4 | 2.8 | 19 | C 13× |
| char_scan / byte_scan | 16 / 19 | 78 / 176 | 31 / 36 | wasm byte_scan needs work |
| map_get_set | 5.1 | 19 | 13 | native probe inlined; wasm pays call overhead |
| regex_find | 741 | 980 | 1378 | C 1.9× vs C# |
| json_serialize | 182 | 310 | 1428 | C 7.8× |
| arr_add | 66 | 225 | 475 | autovec fires natively; wasm scalar-ish |
| vec_add | 30 | 76 | 107 | |

Honest findings from the new compute/ARC benches:
- **Tracing GC beats ARC on allocation-churn-shaped workloads** (`binary_trees`,
  `linked_walk`): freeing a whole tree recursively costs per-node release traffic; GC reclaims
  in bulk. Native ARC is competitive per-node but loses end-to-end there. *(Superseded: as of
  Sep 24 2026 `binary_trees` runs in an inferred bump region and `linked_walk` does no RC per
  hop; both are at parity with C#.)*
- Wasm vs native gaps concentrate in RC-heavy + bounds-check paths (`byte_scan`,
  `scratch_arena`, `list_clear_reuse`) — candidate targets for wasm-specific pass tuning.
- Bench-writing pitfalls locked in by construction (see comments in microbenches.dream): pure
  invariant calls get LICM-hoisted, pure tail sums get SCEV-closed-formed by clang, discarded
  pure results get DCE'd — all three silently report `ns_total=0` unless sinks/args vary.

Wasm code sizes for the suite: O3 468.9 KiB (gz 121.5 / br 95.9), Os 396.3 KiB (gz 104.8 /
br 84.7), Oz 395.2 KiB (gz 102.4 / br 82.8).

### After (deferred: Pike SOA, typed JSON, vec inline, StringBuilder store16)

Pike `Threadq.clear` rewinds `count` (no per-Step `Buffer.alloc`). Capture arrays come from a
slot pool; bytecode is parallel `int[]` SOA with `[a-z]`/`\d` inlined in Step. JSON
`parse_int`/`parse_double` are cursor digit loops (no `JsonValue`); keys match the input slice;
arrays use `List.take_array`. Serialize starts `StringBuilder` at 256 bytes; `write_unit` is
`$array_store16`. `Vector.load`/`+`/`store` lower to `v128` (`f32x4.add`, no sret in
`bench_vec_add`). Builder finish copies from a reserved pad word (`$string_from_builder`).

`--release` WAT: `$RegexVM_find` has no `array_new`; `$JsonParser_parse_int` has no `JsonValue`;
`$bench_vec_add` has `f32x4.add` and 0 sret. Same host as `./scripts/run-microbenches.sh`.

| Bench | Dream | C# | vs prior Dream / vs C# |
|-------|------:|---:|------------------------|
| regex_find | 9419 | 770 | ~10–12k → 9419 (Pike vs Compiled; C# 12×) |
| json_deserialize | 2023 | 1669 | ~3.5–5.3k → 2023 (C# 1.2×) |
| json_serialize | 1293 | 925 | ~1.4k → 1293 (C# 1.4×) |
| vec_add | 259 | 77 | ~260 (WASM 4-wide vs AVX `Vector.Count` often 8; C# 3.4×) |
| string_builder | 23 | 15 | ~24 → 23 (C# 1.5×) |

### After (wrap-by-default integer arithmetic, Sep 2026)

Integer arithmetic wraps by default; `checked { }` opts in to overflow panics. With checked-by-default,
every `+ - *` emitted `__builtin_*_overflow` plus a panic branch, which blocked LICM, ABC and
vectorization in counted loops. The `--release` C for the suite now has no overflow builtins, and
`matmul_64` hoists `i*n` / `k*n` out of the inner loop again. Native C vs C# on the same host
(load was high for this run, so C# numbers are ~30% above the previous run — compare ratios):

| Bench | checked default (Dream / C#) | wrap default (Dream / C#) |
|-------|-----------------------------:|--------------------------:|
| matmul_64 | 542k / 181k (C# 3.0×) | 343k / 250k (C# 1.4×) |
| sieve | 8.7k / 4.6k (C# 1.9×) | 10.1k / 6.5k (C# 1.6×) |
| byte_scan | 66 / 26 (C# 2.6×) | 74 / 36 (C# 2.1×) |
| char_scan | 31 / 21 (C# 1.5×) | 37 / 31 (C# 1.2×) |
| wordcount | 22 / 13 (C# 1.7×) | 24 / 25 (even) |
| string_builder | 14 / 8 (C# 1.8×) | 16 / 52 (Dream 3.4×) |

### After (int32 locals, affine ABC, niche-option hop, Sep 2026)

Native `int`/`uint` locals are `int32_t` again, except locals that are assigned a pointer-sized
value (closure env, task id), which stay `int64_t` so the bits are not truncated. ABC treats
`i << 6 + k` (the algebraic form of `i * 64 + k`, including a loop latch that increments `k`)
and `i * i < len` as in-range. Hop elision cancels the arm-binding retain on a niche
`Option` copy (`tmp = curr#field; node = tmp`), including the loop-carried release.

`--release` C for the suite:

- `bench_matmul` inner `j` loop and `a[i * n + k]` are `dream_p(arr) + 4 + idx * 8`. The only
  `dream_array_at` left is the `c[0]` sink.
- `bench_sieve` marking loop has no `dream_array_at`. *(No longer true as of Sep 24 2026: that
  proof relied on an unsound non-negativity rule, see below.)*
- `bench_linked_walk` arm has no `dream_retain(node)`. Each hop releases the previous `curr`
  after loading `node.next`, then retains the successor (that retain is the cursor's own hold).
  *(Sep 24 2026: the hop now has no retain or release at all.)*

Same host, `REPS=5`. Load was high: most rows are flagged `!`, and C# `matmul_64` landed far
above its previous ~180–250k, so compare Dream's own deltas as well as the ratio.

| Bench | wrap-default Dream | this run Dream min / median | C# min / median |
|-------|-------------------:|----------------------------:|----------------:|
| matmul_64 | 343k | 100k / 109k | 428k / 741k |
| sieve | 10.1k | 2.9k / 3.7k | 7.9k / 10.0k |
| linked_walk | (2 retains + 2 releases / hop) | 3.1k / 3.3k | 2.5k / 3.1k |

Dream's matmul and sieve mins are ahead of this run's C# and ahead of the previous Dream.
`linked_walk` is about even on the median (C# min still ~1.25×). `byte_scan` and
`binary_trees` were not the target of this pass and still favor C# on this host.

### After (perf audit: runtime, BCE rewrite, devirt, RC/escape passes — Sep 24 2026)

What changed (compiler internals: [`05-writing-passes.md`](../../docs/internals/05-writing-passes.md),
[`11-swift-like-arc-roadmap.md`](../../docs/internals/11-swift-like-arc-roadmap.md)):

- **Runtime:** one-word RC encoding (count / atomic / immortal, one load + sign test), per-thread
  alloc counters, inline freelist pop/push, finer native size classes (16 B steps to 256, then
  8 per power of two), iterative destroy glue, target-indexed weak registry (frees of
  non-weak-target objects never touch it), cached string hashes.
- **Bounds checks:** `passes/abc/` rewrite with position-precise facts, `List`/`Queue` element
  access through `Buffer.*_unchecked`, loop versioning for loops bounded by a non-length, and
  `foreach` over `List` lowered as an index loop.
- **Dispatch:** exact-type devirtualization between inliner rounds; tag-guarded direct calls for
  interface slots with ≤2 implementors.
- **RC / allocation:** borrow inference for read-only sink params, type-level mod-ref summaries,
  loop-carried cursor families, post-inline `rc-held-by-owner`, escape analysis +
  `frame-alloc` (stack-built objects with an immortal count), `sroa-managed`.
- **Tooling:** clang PGO (`--profile` / `--use-profile`) — not used for the table below.

Bench changes, so older tables are not comparable on these rows: `iface_dispatch` now calls
through a 1024-entry list of **four** implementors in LCG order (one op = one call), so neither
exact-type nor guarded devirt applies and the itable path is measured; `weak_tree` is new (build
and drop a 1023-node tree with `weak` parent pointers). The HEAD column was built from the
same, new bench source.

Method: HEAD (8175f4e3) and this tree built side by side (HEAD in a `git worktree`), `--release`
native C with zig cc (`-O3`, no LTO on macOS), **7 interleaved reps each** (HEAD, new, HEAD, …)
so drift hits both equally. C# is the min of a separate `REPS=5` run. The machine was loaded:
1-minute load 9.9 at the start and 5.9–8.6 during the interleaved runs (5-minute average up
to ~12.8). All values are **min ns/op**.

| Bench | Dream HEAD | Dream new | new/HEAD | C# |
|-------|-----------:|----------:|---------:|---:|
| nbody | 63.5 | 55.5 | 0.87 | 53.2 |
| mandelbrot | 90.5k | 90.7k | 1.00 | 99.1k |
| matmul_64 | 44.4k | 43.4k | 0.98 | 179.4k |
| quicksort | 19.3k | 18.2k | 0.94 | 37.6k |
| sieve | 1.2k | 3.1k | **2.52** | 4.5k |
| fib_rec | 24.5k | 23.7k | 0.97 | 24.3k |
| iface_dispatch | 7.46 | 6.66 | 0.89 | 7.30 |
| binary_trees | 51.0k | 32.5k | 0.64 | 32.0k |
| linked_walk | 1.3k | 1.2k | 0.90 | 1.1k |
| weak_tree | 4070k | 35.8k | 0.01 | 70.1k |
| wordcount | 19.3 | 11.5 | 0.59 | 17.6 |
| parse_ints | 3.95 | 4.00 | 1.01 | 4.50 |
| sum_options | 0.55 | 0.60 | 1.09 | 1.70 |
| arc_locals | 15.3 | 8.75 | 0.57 | 9.00 |
| string_concat | 17.8 | 11.4 | 0.65 | 9.90 |
| string_eq | 2.88 | 2.40 | 0.83 | 2.80 |
| char_scan | 23.0 | 24.5 | 1.07 | 22.9 |
| byte_scan | 61.5 | 62.0 | 1.01 | 20.9 |
| substring | 1.40 | 1.50 | 1.07 | 6.10 |
| string_builder | 12.2 | 12.5 | 1.02 | 13.3 |
| list_push | 0.85 | 0.55 | 0.65 | 1.10 |
| list_insert_mid | 7.46 | 5.71 | 0.77 | 15.2 |
| map_get_set | 5.75 | 5.90 | 1.03 | 8.70 |
| map_clear_reuse | 3.31 | 3.05 | 0.92 | 2.80 |
| list_clear_reuse | 0.65 | 0.45 | 0.69 | 1.10 |
| alloc_churn | 11.1 | 6.95 | 0.63 | 7.30 |
| scratch_arena | 0.80 | 0.55 | 0.69 | 1.60 |
| regex_find | 594 | 522 | 0.88 | 679 |
| json_serialize | 184 | 150 | 0.81 | 584 |
| json_deserialize | 728 | 524 | 0.72 | 1.7k |
| arr_add | 51.0 | 42.5 | 0.83 | 344 |
| vec_add | 20.5 | 20.0 | 0.98 | 74.9 |

Notes, honestly:

- **sieve regressed 2.5× on purpose.** HEAD removed the marking loop's checks by treating any
  wrapping `Add`/`Mul` of non-negatives as non-negative, which is unsound under wrap-by-default
  arithmetic (`m = m + i` can wrap negative and index out of bounds unchecked). The new facts
  engine only accepts `+1` induction steps, so `m = m + i` keeps its bounds check. Still ahead
  of C#. A follow-up could prove strided increments whose step and guard are both bounded well
  below `INT32_MAX` (so the add provably cannot wrap).
- `binary_trees` now runs inside an inferred bump region (`dream_region_enter` / `leave` in the
  emitted C; HEAD had none) — at parity with C#, closing the gap noted in the Aug 2026 section.
- `linked_walk`'s hop is two loads and an add: no retain or release per hop, and no retain of
  `head` per outer iteration. C# is still ~10% ahead on min here.
- `weak_tree` shows the old global weak list's quadratic teardown (4 ms/op); the registry is
  ~2× faster than C#.
- Rows within ±5% (`mandelbrot`, `matmul_64`, `parse_ints`, `sum_options`, `char_scan`,
  `byte_scan`, `substring`, `string_builder`, `map_get_set`, `vec_add`) are noise at this load;
  sub-ns rows (`sum_options`, `list_push`, `list_clear_reuse`, `scratch_arena`) move in 0.05 ns
  timer steps. C# rows swung widely between reps under this load; treat the C# column as
  indicative only.
- The `byte_scan` C# cell in the table above (20.9 ns) walked code units (`s[j]` over `s.Length`), half of Dream's payload-byte trip count, so the ~3× gap was the bench. C# now walks the UTF-16 LE bytes (`MemoryMarshal.AsBytes`), the same accesses as Dream `byte_at`. One Release run of that loop reported 39 ns/op against Dream's 62 ns min above. The scan now hoists the payload pointer (`dream_str_bytes` once per outer iteration, then a raw byte load). A matching loop with the sink call timed 15 ns/op. Re-run `./scripts/run-microbenches.sh` before replacing the table cell.

Native C is the default `dream run` path: see
[`docs/internals/14-dual-backend-plan.md`](../../docs/internals/14-dual-backend-plan.md).
Do not revive the abandoned LLVM branch for this scoreboard.

Raw logs: `out/native.txt`, `out/csharp.txt`, `out/compare.txt`.

```bash
./scripts/run-microbenches.sh
# or
dream --release run tests/bench/microbenches.dream
# C# only:
cd tests/bench/csharp && DREAM_SCORES=../out/native.txt dotnet run -c Release
```
