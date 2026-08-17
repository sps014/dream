# Microbench baseline and C# parity notes

Recorded with `./scripts/run-microbenches.sh` (Dream `--release` under wasmtime, optional
`dotnet run -c Release` from `tests/bench/csharp`). Absolute values vary by host — use
relative deltas. Dream and C# are **different substrates** (Wasm+ARC vs native JIT+GC);
ratios are not an ARC-only scoreboard.

## API fairness notes

| Bench | Dream | C# |
|-------|-------|-----|
| `char_scan` | Linear UTF-8 scalar decode (`utf8_decode_at` / width) | `foreach` over UTF-16 chars |
| `byte_scan` | `byte_at` walk | UTF-16 code-unit indexer (ASCII-fair) |
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

Native LLVM path (floor for scan/substring/regex): see
[`docs/internals/14-dual-backend-plan.md`](../../docs/internals/14-dual-backend-plan.md)
(branch `llvm` / worktree). Do not revive that backend for this scoreboard.

Raw logs: `out/native.txt`, `out/csharp.txt`, `out/compare.txt`.

```bash
./scripts/run-microbenches.sh
# or
dream --release run tests/bench/microbenches.dream
# C# only:
cd tests/bench/csharp && DREAM_SCORES=../out/native.txt dotnet run -c Release
```
