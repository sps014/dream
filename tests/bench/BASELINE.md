# Microbench baseline — allocator hot paths

Recorded with `dream --release run tests/bench/microbenches.dream`. Absolute values
vary by host — use relative deltas. Re-run via `./scripts/run-microbenches.sh`.

What each bench stresses:

| Bench | Allocator angle |
|-------|-----------------|
| `list_push` / `list_insert_mid` | GPA growth of `List` backing stores |
| `list_clear_reuse` / `map_clear_reuse` | `clear()` keeps capacity — prefer this over allocating a new collection each iteration |
| `map_get_set` | GPA + hashing |
| `alloc_churn` | overwrite-drop of unique `T[]` so the freelist is reused |
| `scratch_arena` | `with ArenaAllocator()` bump for a scope |
| `gc_locals` / `string_concat` / `string_builder` | short-lived strings (GPA vs builder reuse) |
| `string_eq` / `char_scan` / `substring` | string data, little allocation |
| `regex_find` | compile + search; many small heap objects |

```bash
./scripts/run-microbenches.sh
# or
dream --release run tests/bench/microbenches.dream
```

Latest native wasmtime output: `tests/bench/out/native.txt`.
