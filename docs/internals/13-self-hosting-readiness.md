# Self-hosting readiness

Task 7.8 / BOOT-1, reassessed on 2026-10-05.

The four foundation blockers recorded in BOOT-1 are resolved. Dream is ready for a
native bootstrap implementation to begin; it is not yet self-hosted. The compiler
front end, typed representations, optimization passes, LLVM emitter and driver
are still Rust implementations. No Dream-written compiler or bootstrap fixed-point
result is present in this repository.

This reassessment follows the request to take the next Phase 7 task. It evaluates
the implemented desktop foundations without resuming the Android/iOS validation
hold. Phase 6 mobile release readiness remains unverified; it is not a prerequisite
for developing a compiler that runs on the supported desktop host.

## Prerequisite work

Complete the [build performance and minimal compiler plan](./14-build-performance-and-native-networking-plan.md)
before beginning the compiler bootstrap implementation. That work is planned, not
implemented: it prioritizes faster development builds and removing deferred
GPU, networking, WebView, and desktop features while preserving core behavior.
Android/iOS validation remains on hold; artifact sizes are reported without fixed
size assertions.

## Evidence for the original blockers

- **FFI-2, pointer-sized integers:** `isize` and `usize` follow the selected target.
  [Target tests](../../tests/target_spec.rs) check literal ranges, and the
  [pointer-integer golden](../../tests/cases/pointer_integers.dream) exercises
  arithmetic, layout, boxing, collections, closures and async values on native
  and wasm32. These types permit sizes and offsets to cross C boundaries without
  encoding the host's pointer width as a language-level `int`.
- **ABI-1, native address limits:** native references use LLVM pointers; wasm32
  references remain linear-memory offsets. The
  [emission tests](../../tests/native_pointer_emission.rs) distinguish those
  representations. The [machine-size harness](../../tests/runtime_machine_size.rs)
  exercises allocations beyond the former 2 GiB boundary without committing
  large physical buffers, including arrays, strings, futures and regions. That
  harness runs on 64-bit Unix; its scope must not be presented as a Windows
  allocation stress result or a wasm64 implementation.
- **MOB-1, library output:** static and shared library outputs have generated C
  headers and explicit exported entry points. The
  [library consumer tests](../../tests/library_outputs.rs) build and run real C
  consumers, exercise ownership and C calls, verify optional host binding, and
  check relocation of IR and headers. A bootstrap component can therefore be
  embedded through the existing C ABI rather than requiring a new runtime.
- **GEN-1, unstable symbols:**
  [structural symbol encoding](../../crates/dream-types/src/symbols.rs) uses
  declaration paths and structural type arguments rather than interner allocation
  order. Its unit tests and the
  [module identity tests](../../tests/structural_symbols.rs) cover unrelated
  declarations and module name collisions. Exported C names are checked by the
  library consumers. This is reproducible identity, not a promise of a permanent
  pre-1.0 ABI.

LLVM can remain an external pinned toolchain. The existing
[process golden](../../tests/cases/process_run_basic.dream) exercises spawning,
stdout capture, successful exit and missing-executable errors. This establishes
an available process API; it does not prove a Dream compiler's complete toolchain
orchestration, linker argument construction or packaging.

## Remaining implementation and acceptance gates

1. **Specify the bootstrap contract.** Choose one supported desktop host first.
   Record the Rust stage-0 revision, pinned LLVM version, compiler sources,
   embedded stdlib sources, target specification and build options. Reuse the C
   guest runtime and existing host capability boundaries. Document any language
   subset used for intermediate development; a subset compiler cannot satisfy
   the final gate until it accepts all of its own sources.
2. **Implement the Dream compiler.** Port source loading, diagnostics, lexing,
   recovering parsing, interned types, semantic analysis with fused HIR emission,
   MIR lowering, ownership and region verification, optimization and textual LLVM
   emission. Preserve total lowering boundaries and deterministic registries.
   Mature C libraries may be reached through the existing FFI where useful;
   implementing Rust dependencies afresh is not automatically required.
3. **Complete the native driver.** Exercise the actual pinned LLVM tools and
   linker through Dream's process API, including arguments containing spaces,
   failed commands, diagnostics, runtime compilation and cache invalidation.
   Validate generated executables and library outputs with existing consumers.
   Successful process spawning alone does not close this gate.
4. **Establish semantic parity.** Run the full golden corpus through the
   Dream-written compiler, including diagnostics and traps. Compare native and
   supported wasm32 execution, ownership/region invariants, C/C++ interop and
   emitted ABI contracts. Keep Rust stage 0 as the explicit bootstrap seed until
   replacement passes these gates; do not add fallback compilation paths that
   conceal unsupported programs.
5. **Prove the bootstrap fixed point.** Stage 0 compiles the Dream compiler to
   stage 1; stage 1 compiles the same compiler sources to stage 2; stage 2 repeats
   the build to stage 3. Use identical target, toolchain, options and output paths
   in separate clean runs. Require byte-identical stage-2/stage-3 compiler
   artifacts and repeat-compilation LLVM/ABI outputs. Record source and artifact
   hashes and retain logs. Explain any nondeterministic packaging metadata and
   remove it from the build rather than normalizing away generated-code
   differences. Both generated compilers must independently pass semantic parity.
6. **Measure and adopt.** Record bootstrap time, peak memory and compiler/runtime
   artifact sizes alongside the Rust seed, using reproducible release settings.
   Report changes without fixed size assertions so future features can grow.
   Switch the default compiler only after correctness, deterministic bootstrap
   and supported desktop platform gates pass. Expand the bootstrap host matrix
   separately; mobile release validation remains subject to its existing hold.

The readiness review is complete when the obsolete blocker list is corrected and
these remaining gates are explicit. Actual self-hosting requires all six gates;
none is claimed complete by this assessment. Phase 7 also retains its independent
file-size hygiene and metric exit criteria.
