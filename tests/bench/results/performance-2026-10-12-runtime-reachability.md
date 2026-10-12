# Native runtime reachability, 2026-10-12

The targeted cost is startup argument/path capture, callback-owner creation and cleanup,
unconditional embedding roots, and deferred-cleanup storage in programs that need none of them.
Preserved compiler/runtime pairs and output checksums are recorded in the adjacent JSON.

On the local Apple arm64 runner, the empty executable drops from 108 to 19 symbols and from
53,784 to 50,296 bytes. Its text section shrinks to 1,628 bytes, and its thread-variable and
thread-BSS sections disappear. Print, arguments and compute programs also become smaller;
all four produce the reference output. Argument/path queries retain capture; reachable C
imports and native library outputs retain embedding exports. Leak reporting remains available.

An exploratory 20-round launch-to-exit run before the final deferred-cleanup change passes the
10% regression gate on all four fixtures. A predefined follow-up waits for compiler activity to
settle for 30 seconds, then uses 20 rounds and ten passes with alternating execution order.
All four rows pass the 10% gate, with no supported launch speedup. Current/reference ratios are 1.034 for arguments (95% CI 0.982–1.066), 0.976 for compute (0.942–1.030), 1.004 for empty (0.944–1.026), and 0.994 for print (0.929–1.020). This remains local exploratory evidence. Process launch timing includes loading, startup, workload and teardown;
it does not establish a phase-specific speedup or replace application throughput measurements.

This campaign covers startup and roots. Full runtime-unit dependency selection, the complete
service/rejection diagnostic taxonomy, and hardware/platform acceptance remain outstanding.
