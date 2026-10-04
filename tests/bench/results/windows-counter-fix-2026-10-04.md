# Windows heap-counter regression fix — 2026-10-04

Intel Core i9-13900K, pinned LLVM 22.1.8, Dream release/O3. One warmup process, five measured processes, five passes each, using the unchanged `scripts/run-microbenches.sh` under Git Bash. All benchmark runs finished before workspace validation began.

Native allocation/free counters are written only by their owning thread. 64-bit relaxed atomic loads/stores preserve race-free diagnostic snapshots without locked read-modify-write instructions per allocation. Shared wasm32 counters retain atomic read-modify-write updates. Tests perform 800,000 real allocations across eight workers while reading snapshots, verify exact totals after joining, and exercise totals above the 32-bit boundary.

Baseline is `09d53af5`; the regressed run is the implementation in `125c9f1a`. The fixed run uses this follow-up implementation. These are sequential runs, not an interleaved experiment. Percentages compare the fixed run with baseline; positive means slower. Spread is (max−min)/median of process medians. An exclamation mark indicates spread above 15%; zero timings are unresolved.

| Benchmark | Baseline ns/op | Regressed ns/op | Fixed ns/op | Fixed vs baseline | Fixed spread |
|---|---:|---:|---:|---:|---:|
| alloc_churn | 4.9 | 10.0 | 4.7 | -4.8% | 4% |
| arc_locals | 5.7 | 10.8 | 5.5 | -3.6% | 3% |
| arr_add | 20.3 | 17.8 | 18.6 | -8.5% | 15%! |
| binary_trees | 29,534.0 | 30,909.0 | 28,321.0 | -4.1% | 8% |
| byte_scan | 7.6 | 7.4 | 7.3 | -3.5% | 6% |
| char_scan | 7.4 | 7.1 | 7.0 | -4.8% | 6% |
| fib_rec | 16,911.4 | 17,055.3 | 16,981.9 | +0.4% | 5% |
| iface_dispatch | 0.6 | 0.6 | 0.8 | +29.6% | 7% |
| json_deserialize | 382.6 | 487.7 | 374.6 | -2.1% | 6% |
| json_serialize | 124.6 | 150.6 | 120.0 | -3.7% | 9% |
| linked_walk | 958.0 | 924.5 | 920.0 | -4.0% | 7% |
| list_clear_reuse | 0.5 | 0.5 | 0.4 | -4.2% | 23%! |
| list_insert_mid | 4.9 | 5.0 | 4.6 | -5.4% | 6% |
| list_push | 1.1 | 1.1 | 1.1 | -2.2% | 10% |
| mandelbrot | 73,841.0 | 70,888.0 | 70,959.5 | -3.9% | 3% |
| map_clear_reuse | 1.5 | 1.5 | 1.5 | -3.3% | 5% |
| map_get_set | 2.1 | 2.1 | 2.0 | -7.1% | 8% |
| matmul_64 | 22,132.8 | 21,309.5 | 21,159.8 | -4.4% | 9% |
| nbody | 31.7 | 30.4 | 30.4 | -4.1% | 4% |
| parse_ints | 0.0 | 0.0 | 0.0 | Unresolved | 0% |
| quicksort | 15,218.4 | 14,727.4 | 14,323.2 | -5.9% | 2% |
| regex_find | 720.1 | 714.9 | 698.4 | -3.0% | 4% |
| scratch_arena | 0.0 | 0.0 | 0.0 | Unresolved | 0% |
| sieve | 1,534.9 | 1,461.8 | 1,416.1 | -7.7% | 4% |
| string_builder | 17.1 | 15.9 | 16.4 | -4.5% | 11% |
| string_concat | 3.9 | 3.9 | 3.9 | -1.5% | 7% |
| string_eq | 1.4 | 1.4 | 1.3 | -5.8% | 7% |
| substring | 0.0 | 0.0 | 0.0 | Unresolved | 0% |
| sum_options | 0.0 | 0.0 | 0.0 | Unresolved | 0% |
| vec_add | 12.2 | 12.1 | 11.7 | -4.0% | 12% |
| weak_tree | 63,825.0 | 73,046.0 | 58,393.0 | -8.5% | 6% |
| wordcount | 7.1 | 7.0 | 7.1 | +0.6% | 5% |
