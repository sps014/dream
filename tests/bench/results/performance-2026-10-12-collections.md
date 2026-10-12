# Dream performance campaign: collections — 2026-10-12

Local macOS arm64 exploration: 20 alternating process rounds per compiler, two warmup rounds, five measured passes per process, uninstrumented timing, bootstrap 95% confidence intervals. This is not dedicated-runner or cross-platform acceptance. Checksums match in every workload.

- `list_bulk_1048576`: current/reference 0.984, 95% CI 0.936–1.001; pass.
- `list_bulk_131072`: current/reference 0.894, 95% CI 0.875–0.926; pass.
- `list_bulk_256`: current/reference 0.405, 95% CI 0.402–0.410; pass.
- `list_bulk_8`: current/reference 1.000, 95% CI 1.000–1.069; pass.
- `list_bulk_8192`: current/reference 0.552, 95% CI 0.542–0.557; pass.
- `list_managed_clear_256`: current/reference 0.820, 95% CI 0.808–0.824; pass.
- `list_managed_clear_8`: current/reference 0.676, 95% CI 0.667–0.676; pass.
- `list_managed_pop_256`: current/reference 0.713, 95% CI 0.707–0.718; pass.
- `list_managed_pop_8`: current/reference 0.676, 95% CI 0.672–0.694; pass.
- `list_reserve_8192`: current/reference 0.431, 95% CI 0.426–0.441; pass.
- `map_set_churn`: current/reference 0.899, 95% CI 0.890–0.902; pass.

Peak process RSS: reference 14680064 bytes, current 11042816 bytes. RSS is process-level and does not isolate allocator retention.

Raw samples, binaries, fixture copies and measurement identity are preserved under `target/perf-plan/collection-accepted-timing`. Compiler and matching runtime snapshots are under `target/perf-plan/baseline` (collections reference) and `target/perf-plan/current-snapshot` (text reference). The checked-in JSON contains process samples and measurement metadata.

Collection executable sizes are 125264 bytes for both arms. The initial tombstone compaction experiment regressed churn by 23–25% and was removed. Its rejected samples remain under `target/perf-plan/collection-timing`. Managed value stores and inline union layout fixes were correctness prerequisites.
