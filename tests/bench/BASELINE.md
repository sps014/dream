# Microbench baseline and C# parity notes

Recorded with `./scripts/run-microbenches.sh` (Dream `--release` native C, optional
`dotnet run -c Release` from `tests/bench/csharp`). Absolute values vary by host — use
relative deltas. Dream and C# are **different substrates** (Wasm+ARC vs native JIT+GC);
ratios are not an ARC-only scoreboard.

## API fairness notes

| Bench | Dream | C# |
|-------|-------|-----|
| `char_scan` | `char_at` loop over UTF-16 code units | `foreach` over UTF-16 chars |
| `byte_scan` | `byte_at` walk of UTF-16 LE payload | UTF-16 code-unit indexer (ASCII-fair) |
| `substring` | `substring(start, end)` | `Substring(start, length)` |
| `scratch_arena` | `bump` / `set_at` / `at` (no Span RC) | same index API |
| `regex_find` | Global `[a-z]+\d+` via Pike VM (not bare `\d+`) | same pattern, `Compiled` |
| `json_serialize` / `json_deserialize` | Nested `@json` User+Address, payload built once; deserialize text outside timer; scale `/10` | `System.Text.Json` (no source-gen) |
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
fills `T` from `JsonParser` (`from_json_parser_text`); `Json.parse` / `from_json` stay for
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
> pointer itself (`None` = null). Effects vs the table below: **binary_trees 190k → ~70-100k
> ns/op (2-2.8×, at or better than C# 72k)**, json_deserialize 965→~900, regex_find 741→~700,
> string/alloc rows all slightly better; linked_walk unchanged (~4-5k, C# 1.7k still ahead on
> pointer chasing). The old headline finding "tracing GC beats ARC on tree churn" no longer
> holds natively.

Same host, `./scripts/run-microbenches.sh` — now a **three-way** table: Dream native C
(cc -O3 LTO), Dream wasm32 under Node (`--wasm --release --runtime --node`), C# RyuJIT.
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
| iface_dispatch | 3.2 | 21 | 32 | devirt+inline wins; wasm dispatch cost visible |
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
  in bulk. Native ARC is competitive per-node but loses end-to-end there.
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
