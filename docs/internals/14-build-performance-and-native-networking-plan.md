# Faster development builds and a minimal compiler before self-hosting

**Status: Planned — prerequisite to self-hosting.**

Recorded on 2026-10-05. This document specifies future work; implementation and
acceptance gates have not been completed.

## Summary

Prioritize both Dream edit-run latency and compiler/host rebuild time. Remove GPU, networking, WebView, and desktop features to simplify the compiler
and runtime.

[Self-hosting](./13-self-hosting-readiness.md) follows this work. Moving the compiler
into Dream follows simplification of the current compiler and runtime.

## Faster build workflows

- Measure separate baselines: cold toolchain build, compiler-only rebuild, host-only
  rebuild, unchanged Dream build, one-file Dream edit, and release compilation.
  Record phase timings, peak memory and dependency rebuilds.
- Make the local development setup build the compiler and essential hosts by
  default. Add explicit capability selection and an all-capabilities option;
  Removed capabilities must not be prerequisites for compiler development.
- Preserve full workspace validation and release packaging as comprehensive
  workflows. Cache their dependencies independently from targeted development
  builds.
- Introduce an internal `CompileProfile` shared by Dream and Dreamer:
  - **Dev:** default compile/run; essential lowering and ownership insertion, one
    inexpensive cleanup round, LLVM `O0`, no whole-module inlining or expensive
    optimization fixpoints.
  - **Release:** preserve the existing complete MIR and LLVM optimization pipelines.
  - **Debug:** preserve debugger-specific behavior and DWARF.
- Keep explicit optimization flags authoritative; `-O0` uses Dev behavior and
  higher explicit levels use the optimizing pipeline. Preserve existing debug-info
  precedence.
- Dev cleanup uses copy propagation, constant folding, CFG simplification,
  dead-code elimination and RC elision. Async poll cleanup omits transformations
  unsafe across suspension.
- Include the effective profile in cache keys. Update optimization-specific tests
  to request Release explicitly.
- Move verified unchanged-build cache lookup ahead of parsing and generator
  execution. Persist the complete input graph, resolution-directory fingerprints,
  options, tool identities and artifact hashes. Incomplete dependency information
  forces a cache miss.
- Preserve conservative cache behavior for native inputs until C/C++ source and
  header dependencies are recorded correctly. Do not add an incremental-compilation
  claim without evidence.

## Minimal compiler and runtime

Remove GPU, networking, WebView, and desktop features across native and WASM.
Delete their hosts, stdlib packages, shader and HTTP route generators, JS chunks,
and dependencies. Future services are deferred; no C networking replacement is
part of this work. Retain core services, native/WASM compilation, async/tasks,
and general JS/native interop. Binaryen remains as a pinned executable installed
through Dreamer, outside Cargo; optimized WASM builds install it on first use.

## Tests and acceptance

- Run the full golden corpus in Dev and Release, including diagnostics, traps,
  destructor timing, async cancellation and ownership verification.
- Verify release optimization fixtures against the baseline; changing development
  defaults must not reduce release optimization.
- Test cache hits, edited imports, changed manifests, added resolution candidates,
  changed tools, corrupt artifacts and failed builds.
- Verify retained host selection, relocated CLI packs, and core-only programs.
- Compare build times and memory
  against retained baseline artifacts. Use repeated trials and publish raw
  results; do not claim gains from architectural changes alone.
- Require measurable improvement in targeted development workloads, passing
  core behavior checks and all repository gates. Report artifact sizes without fixed
  size assertions.

## Delivery order and defaults

1. Remove deferred capabilities and their integration points.
2. Validate the retained compiler, runtime, and tooling.
3. Measure targeted build workflows.
4. Pursue Dev profiles and earlier cache lookup separately.

Rust remains in the compiler until the later self-hosting project. GPU,
networking, WebView, and desktop UI may return through future scoped work.
