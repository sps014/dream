# Windows stdlib performance follow-up — 2026-10-04

Intel Core i9-13900K; Dream native release/O3, LLVM 22.1.8; C# Release on .NET 10. The unchanged runner used one warmup process and five measured processes of five passes each. The runner completed before validation; the paired comparison ran after validation. Neither overlapped builds or tests.

Global regex matching batches offsets with match state local to one call, skipping per-match wrapper/list allocations. List constructors expose initial capacity and count to optimization. StringBuilder private buffers skip zero-fill because only initialized bytes below count are read; build still copies to an independent immutable string. Slice bounds are checked before copying.

## Alternating Dream comparison

The baseline binary was preserved from `dfa48635`. Six paired process runs alternate old/new order, each with an internal warmup and five measured passes. Values below are medians of per-process medians; change is the median paired ratio. Positive means slower. These runs help control frequency/load drift but are still single-machine microbenchmarks; zero timings cannot establish speedups.

| Benchmark | Baseline ns/op | Updated ns/op | Paired change |
|---|---:|---:|---:|
| nbody | 31.2 | 31.1 | +0.4% |
| mandelbrot | 72,574.8 | 71,417.5 | -1.7% |
| matmul_64 | 21,681.6 | 21,482.9 | -0.6% |
| quicksort | 14,868.9 | 15,025.0 | +1.1% |
| sieve | 1,517.7 | 1,473.0 | -2.9% |
| fib_rec | 17,419.1 | 17,107.8 | -2.3% |
| iface_dispatch | 0.8 | 0.8 | +0.3% |
| binary_trees | 29,669.5 | 29,087.5 | -2.0% |
| linked_walk | 942.0 | 932.3 | -1.1% |
| weak_tree | 61,387.0 | 61,161.0 | -0.6% |
| wordcount | 7.3 | 7.1 | -1.8% |
| parse_ints | 0.0 | 0.0 | Unresolved |
| sum_options | 0.0 | 0.0 | Unresolved |
| arc_locals | 5.7 | 5.5 | -1.9% |
| string_concat | 3.9 | 3.9 | -1.0% |
| string_eq | 1.4 | 1.5 | +4.6% |
| char_scan | 7.1 | 7.1 | +0.9% |
| byte_scan | 7.4 | 7.4 | +0.6% |
| substring | 0.0 | 0.0 | Unresolved |
| list_push | 1.1 | 0.9 | -19.6% |
| list_insert_mid | 4.7 | 4.7 | -0.6% |
| map_get_set | 2.0 | 2.0 | -0.8% |
| map_clear_reuse | 1.5 | 1.5 | +1.7% |
| list_clear_reuse | 0.5 | 0.2 | -53.6% |
| alloc_churn | 4.8 | 4.9 | +0.6% |
| scratch_arena | 0.0 | 0.0 | Unresolved |
| regex_find | 702.8 | 285.9 | -59.1% |
| string_builder | 17.5 | 14.2 | -16.7% |
| json_serialize | 121.6 | 122.4 | +3.0% |
| json_deserialize | 379.6 | 362.3 | -5.0% |
| arr_add | 18.5 | 19.5 | +3.7% |
| vec_add | 12.6 | 12.4 | -1.9% |

## Current Dream/C# run

Times are median ns/op. Spread is (max-min)/median of process medians; ! means above 15%. Regex compares public APIs: Dream returns strings, C# reads Match.Length without copying matched text. Binary trees include immediate ARC destruction in Dream and permit deferred GC in C#. The separate reclaim row forces full C# GC inside the timer, after a clean heap beforehand, and runs last. Its synthetic collection overhead is not a general ARC/GC comparison.

| Benchmark | Dream ns/op | C# ns/op | Dream spread | C# spread |
|---|---:|---:|---:|---:|
| alloc_churn | 4.8 | 9.3 | 26%! | 6% |
| arc_locals | 5.6 | 10.6 | 9% | 19%! |
| arr_add | 18.4 | 166.2 | 12% | 3% |
| binary_trees | 28,261.0 | 21,930.0 | 8% | 13% |
| binary_trees_reclaim | 28,616.0 | 27,295.0 | 9% | 29%! |
| byte_scan | 7.3 | 34.9 | 4% | 5% |
| char_scan | 7.0 | 11.9 | 12% | 7% |
| fib_rec | 16,982.3 | 19,943.6 | 4% | 5% |
| iface_dispatch | 0.8 | 1.7 | 6% | 12% |
| json_deserialize | 356.2 | 2,613.2 | 6% | 19%! |
| json_serialize | 124.1 | 467.7 | 7% | 13% |
| linked_walk | 945.0 | 936.0 | 8% | 6% |
| list_clear_reuse | 0.2 | 0.5 | 5% | 20%! |
| list_insert_mid | 4.7 | 9.7 | 6% | 11% |
| list_push | 0.9 | 0.8 | 6% | 0% |
| mandelbrot | 70,985.0 | 74,691.5 | 3% | 4% |
| map_clear_reuse | 1.5 | 2.1 | 5% | 10% |
| map_get_set | 2.2 | 7.1 | 20%! | 6% |
| matmul_64 | 21,282.5 | 184,122.2 | 5% | 15%! |
| nbody | 30.2 | 34.4 | 3% | 7% |
| parse_ints | 0.0 | 2.9 | 0% | 7% |
| quicksort | 14,743.1 | 16,585.8 | 5% | 90%! |
| regex_find | 282.7 | 486.4 | 5% | 6% |
| scratch_arena | 0.0 | 0.6 | 0% | 0% |
| sieve | 1,453.3 | 3,098.2 | 9% | 7% |
| string_builder | 15.5 | 15.4 | 11% | 5% |
| string_concat | 3.9 | 8.1 | 13% | 10% |
| string_eq | 1.4 | 1.4 | 11% | 15%! |
| substring | 0.0 | 0.3 | 0% | 0% |
| sum_options | 0.0 | 0.3 | 0% | 0% |
| vec_add | 11.9 | 31.2 | 15% | 4% |
| weak_tree | 62,585.0 | 94,158.0 | 9% | 13% |
| wordcount | 7.1 | 9.8 | 9% | 8% |
