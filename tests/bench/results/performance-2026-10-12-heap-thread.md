# Native heap thread retirement — 2026-10-12

The targeted costs are reusable storage stranded at native thread exit and one counter record retained per retired thread. Worker exit and embedding detach now return class blocks and unused current-arena tails to process ownership. Retired counters fold into fixed process totals. Live shared objects survive thread exit. This is not slab reclamation or memory returned to the OS.

The injected-platform harness runs 64 sequential threads, keeps a shared object live across each exit, verifies its payload, and checks mapping and counter-record allocation growth stays at most two. Concurrent diagnostic tests and AddressSanitizer/UBSan pass.

The initial local macOS arm64 20-round collection run had two inconclusive rows. The predefined follow-up used another 20 alternating rounds, two warmups and ten measured passes; all rows pass the 10% hard gate. Managed pop has a supported roughly 5% slowdown. This tradeoff remains explicit; there is no application speedup claim for this campaign. Other rows show no supported regression above the gate. Dedicated-runner and other-platform acceptance remain pending.

- `list_bulk_1048576`: current/reference 0.958; 95% CI 0.906–1.004; pass.
- `list_bulk_131072`: current/reference 0.964; 95% CI 0.889–1.027; pass.
- `list_bulk_256`: current/reference 0.995; 95% CI 0.989–1.000; pass.
- `list_bulk_8`: current/reference 1.000; 95% CI 0.992–1.000; pass.
- `list_bulk_8192`: current/reference 0.999; 95% CI 0.988–1.002; pass.
- `list_managed_clear_256`: current/reference 1.000; 95% CI 1.000–1.000; pass.
- `list_managed_clear_8`: current/reference 0.995; 95% CI 0.981–1.000; pass.
- `list_managed_pop_256`: current/reference 1.050; 95% CI 1.039–1.051; pass.
- `list_managed_pop_8`: current/reference 1.055; 95% CI 1.030–1.067; pass.
- `list_reserve_8192`: current/reference 0.992; 95% CI 0.958–1.046; pass.
- `map_set_churn`: current/reference 0.996; 95% CI 0.980–1.001; pass.

Both runs and raw samples remain under `target/perf-plan/heap-thread-timing` and `target/perf-plan/heap-thread-followup`. The follow-up uses immutable matching runtime/compiler snapshots under `target/perf-plan/text-snapshot` and `target/perf-plan/heap-thread-snapshot`. The compiler is held identical between arms.
