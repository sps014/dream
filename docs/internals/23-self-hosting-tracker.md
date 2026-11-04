# Self-hosting tracker

This page tracks the remaining work to port the Rust compiler to Dream. The port follows
Bun's Zig-to-Rust migration: every compiler file is translated at once, mechanically. The
drafts are then made to compile one module at a time with the Rust compiler (Stage 0), and
the result is accepted only when it produces byte-identical artifacts on the existing corpus.
The acceptance gates are defined in [Self-hosting readiness](13-self-hosting-readiness.md).
This page records what is left.

Status markers: `[ ]` not started, `[~]` in progress, `[x]` done. Update a line when its work
merges, and link the commit or PR.

## Scope

Ported, about 160k lines of Rust excluding tests:

- `dream-text`, `dream-diagnostics`, `dream-syntax`, `dream-format`.
- `dream-types`, `dream-hir`, `dream-abi`, and the `dream-stdlib` registry.
- `dream-sema`.
- `dream-mir`, including every Release pass.
- `src/driver` and `src/execution`.
- `tooling/dream-lsp` (about 11k lines). It links the compiler front end, so it moves in the
  same wave; no second front end is left behind.

Not ported: the C runtime, `runtime/` JavaScript sources, the `dream-host-*` capability
libraries, and the VS Code extension (TypeScript). The first three are separate C-ABI or
JavaScript components. The extension only starts `dream-lsp`.

## Dependency policy

The Dream compiler has no third-party library dependencies. Each Rust crate is replaced in
this order of preference:

1. A pinned external tool, run as a subprocess. The LLVM distribution already contains every
   tool listed below, but `scripts/fetch-dev-llvm.sh` (`TOOLS`) and the pinned-tool table in
   `crates/dream-abi/src/toolchain.rs` keep only `clang`, `opt`, `llc`, `llvm-link`,
   `llvm-dis`, `llvm-as`, `llvm-ar`, `llvm-profdata`, `llvm-rc`, `lld` and `wasm-ld`. Add
   `llvm-objcopy`, `llvm-readobj`, `llvm-nm` and `llvm-split` there. `esbuild` and `brotli` are
   newly pinned tools, installed like Binaryen.
2. The Dream stdlib.
3. A small amount of compiler code, when the format is simple and Dream-specific.

Vendored C libraries reached through FFI are not used for the compiler.

## Collection usage in the Rust compiler

Line matches across the ported crates. These counts set the priority of the stdlib work below.

| Rust type | Uses | Dominant key types |
|---|---:|---|
| `IndexMap` / `IndexSet` | 411 / 337 | `String`, `TypeId`, `DefId`, tuples of IDs |
| `HashMap` / `HashSet` | 177 / 146 | lookups that never affect output order |
| `BTreeMap` | 239 | `String`, `usize`, `Local`, `DefId`, `Field` |
| `BTreeSet` | 416 | `TypeId`, `BlockId`, `Local`, `DefId`, `usize` |
| `SmallVec` | 64 | `u32` with inline capacity 2–16 |
| `VecDeque` | 25 | worklists |
| `Rc` / `RefCell` | 127 / 62 | shared mutable compiler state |
| `bumpalo` | 148 | AST and HIR arena lifetimes |
| `OnceCell` / `OnceLock` / `LazyLock` | 32 | lazily built tables |
| `Arc` / `Mutex` | 54 / 7 | driver parallelism and caches |

## Stdlib data structures

Existing types (`Map`, `Set`, `Queue`, `SortedMap`, `List`, `StringBuilder`) and their
behavior, layout and hashing are not changed. Every new capability is a new type in its own
file under `system/collections` or `system/text`. Each new type adds a capability the existing
types lack, such as insertion order, positional access, double-ended access, O(log n) sorted
insert or byte-level text, so none of them is an alias under the "one user-facing way per
concept" rule. Each one gets `///` documentation, a reference page and assertion tests.

### Required before Phase A

- [ ] **`OrderedMap<K, V>` and `OrderedSet<T>`.** Insertion-ordered hash tables in the
  `IndexMap` layout.
  - A dense entry array in insertion order, each entry caching its hash.
  - An open-addressing index table of `u32` entry positions.
  - Iteration walks the entry array, so order is deterministic.
  - Positional access (`entry_at(i)`, `index_of(key)`), `swap_remove` and `shift_remove`.
  - Lookup by a borrowed view (`StringKey` and byte-span keys) without building a key.
  - They replace `IndexMap`, `IndexSet`, `HashMap` and `HashSet` (1,071 uses). The existing
    `Map` and `Set` are not used anywhere output order can be observed.
- [ ] **`BitSet`.** A set of dense non-negative integers. It replaces `BTreeSet` keyed by
  `TypeId`, `BlockId`, `Local`, `DefId` or `usize`, and dataflow facts in MIR passes.
  - Storage: a list of 64-bit words.
  - Insert, remove and contains in O(1).
  - Word-at-a-time `union_with`, `intersect_with` and `subtract`, each returning whether the
    set changed.
  - Ascending iteration (matching `BTreeSet` order), count, equality and `hash_code`.
- [ ] **`Deque<T>`.** A double-ended ring buffer (`push_front`, `push_back`, `pop_front`,
  `pop_back`, indexed access) for the `VecDeque` worklists.
- [ ] **`Utf8Text`.** UTF-8 source text, separate from the UTF-16 `string`. The Rust
  compiler's spans, columns, diagnostics and debug locations are UTF-8 byte offsets, and they
  have to stay byte-identical.
  - Owns the file's `byte[]` and hands out `ReadOnlySpan<byte>` views.
  - Byte search (lowers to `memchr`), view equality, and a fast word-at-a-time hash
    (rapidhash or wyhash) in the C runtime, used by the ordered tables for byte-view keys.
  - Lazily built line-start table in byte offsets.
  - Explicit conversion of a view to `string`, used only for identifiers and literals.
- [ ] **`Utf8Builder`.** A byte-based text builder that writes ASCII and UTF-8 directly into a
  growing `byte[]` and flushes to a file. The LLVM emitter writes large `.ll` files, and it
  must not pay a UTF-16-to-UTF-8 conversion for each character.
- [ ] **Bit intrinsics on integer primitives.** New methods `trailing_zeros`, `leading_zeros`,
  `count_ones` and `rotate_left` on `uint`, `ulong` and `usize`. They lower to LLVM `cttz`,
  `ctlz` and `ctpop`, and `BitSet` iteration and hashing need them. Adding these methods leaves
  every existing method unchanged.
- [ ] **Hash mixing in the new tables.** `OrderedMap` and `OrderedSet` apply a Fibonacci
  multiply to `hash_code` before probing, so sequential integer IDs don't cluster. The
  `hash_code` methods of existing types stay as they are.

### Required before Phase B performance work

- [ ] **`TreeMap<K, V>` and `TreeSet<T>`.** B-tree ordered collections with O(log n) insert
  and remove, `first`, `last`, and range iteration. They serve `BTreeMap<String, _>`,
  `BTreeMap<PathBuf, _>`, and any `BTreeSet` whose keys are not dense IDs. The existing
  `SortedMap` (sorted arrays, O(n) insert) stays as it is for small, mostly read tables.
- [ ] **Stable sort check.** The port relies on stable sorting wherever the Rust code calls
  `sort_by`. `List.sort_by` is a stable merge sort, so the port uses it as is. If profiles show
  sorting is slow, the faster sort goes into the new types, not into `List`.

### Phase E only, after the switch (from `PERF(port)` markers)

- [ ] **Arena-backed nodes.** Move AST, HIR and MIR nodes from ARC classes into `Arena` storage
  as unmanaged records addressed by ID. Benchmark each conversion.
- [ ] **`SmallList<T>`.** A list with inline capacity. Add it only if allocation at the 64
  `SmallVec` sites shows up in profiles.

### Already available

- `List`, `Queue`, `Stack`, `PriorityQueue` (four-ary heap), `SortedMap`.
- `StringBuilder`, `Span` / `ReadOnlySpan`, `Bytes`, `Arena`, `Lock`, `Task`.
- `float16` (for the `half` crate) and try-propagation (for `?`).
- `system.cli` (for `clap`), `system.json` (for `serde_json`), `system.config`,
  `system.logging` (for `tracing`), `system.process`, temporary files (for `tempfile`).
- `Assets` for embedding the stdlib `.dream` sources into the compiler binary.

## Prerequisites in Stage 0 (Rust)

- [ ] **Per-module incremental analysis.** Reuse each module's declaration tables and its
  HIR/MIR across edits, keyed by `ModuleHashes::cache_key`
  (`crates/dream-sema/src/module_graph.rs`).
- [ ] **Debug codegen units.** See [Codegen units](#codegen-units).
- [ ] **Release split after optimization.** See [Codegen units](#codegen-units).
- [ ] **Parallel compiler phases.** Function passes run in parallel (see
  [Compiler performance](#compiler-performance)). Debug LLVM emission is still one `llc`.
- [x] **Prelude cache.** Parsed prelude packages are reused for the process, keyed by package
  name, path and the embedded source identity.
- [x] **Self-host benchmark.** `scripts/bench.py build --workload selfhost` compiles an 8,000
  function program and records cold, edited and unchanged times.
- [ ] **IR dumps.** Add stable text dumps of the AST, HIR and MIR, used for diffing in Phase C.
- [ ] **Remove `.wat` output and `wasmprinter`.** The only caller is
  `src/driver/compiler/emit.rs`, which writes a `.wat` next to every wasm build.
  - Delete `crates/dream-mir/src/backend/shared/print.rs` and the `wasmprinter` dependency.
  - Rename `embed_abi_in_wasm(wat_path)` and the JS runtime emission to take the `.wasm`
    path.
  - Move tests that parse emitted `.wat` (the `wat` dev-dependency) to the `.wasm` bytes.
  - Update `AGENTS.md`, `01-pipeline-overview.md` and `06-llvm-backend.md`, and the user docs
    that list the `.wat` artifact.
  - Anyone who wants text can run Binaryen's `wasm-dis` on the `.wasm`.
- [ ] **Swap dependencies for tools while still in Rust.** Each replacement in the table below
  marked "tool" or "precomputed" lands in Stage 0 first, so the port copies a pipeline that is
  already tool-based and the corpus proves it before the freeze.
- [ ] **Update readiness doc.** Rewrite the prerequisite section of
  [Self-hosting readiness](13-self-hosting-readiness.md), which still points at the old minimal
  compiler plan.

## Codegen units

Today the backend emits one `.ll` for the whole program
(`src/execution/llvm/build.rs`):

- **Debug:** the program module goes through `opt` (only with `-O`) and then a single `llc`,
  and is linked with the cached runtime objects. Any edit recompiles the whole program with
  `llc`.
- **Release:** `llvm-link` merges the program with the runtime bitcode, then one whole-program
  `opt` and one `llc` run, both single-threaded.

Splitting into several `.ll` files helps, but it should be done differently per profile.

- [ ] **Debug: one codegen unit per Dream module.**
  - The emitter writes one `.ll` per module.
  - Cross-unit references become `declare`s with external linkage.
  - Monomorphized instances, string constants and type metadata go to the unit that first
    requests them in module order, emitted as `linkonce_odr` (in a COMDAT where the target
    needs it), so the linker removes duplicates.
  - Each unit's object is cached under the hash of its `.ll` text. An edit runs `llc` only for
    the units whose text changed, in parallel, and relinks.
  - This also gives per-module MIR caching an output to reuse.
  - For a 160k-line compiler, a single `llc` at O0 takes several seconds per edit. With units,
    the cost scales with the edited module.
- [ ] **Release: keep one whole-program module through `opt`, then split it for code
  generation.**
  - Whole-program `opt` with internalization is what makes Release fast, so it stays.
  - After `opt`, the pinned `llvm-split` partitions the optimized module, `llc` runs on the
    partitions in parallel, and the objects are linked.
  - Generated code is the same as before, because optimization finished before the split.
    Only code generation is parallel.
  - The partition count is a fixed constant, or a function of module size, never the
    machine's CPU count. This keeps the output binary byte-identical across machines.
- [ ] **Not planned: ThinLTO.** Running `opt` per unit with summary-based importing would
  parallelize Release optimization too, but it weakens whole-program internalization and adds
  linker plugin requirements. Dream's MIR already monomorphizes and inlines across the whole
  program before LLVM, so LLVM's cross-unit inlining matters less. Reconsider it only if
  measurements show whole-program `opt` dominates Release builds of the compiler.
- [ ] **wasm32:** the same split applies. Debug units link with `wasm-ld`; Release keeps one
  module through `opt` and `wasm-opt`.

Determinism requirements for all of the above:

- How code is assigned to units depends only on module order and structural symbol names.
- Each unit's `.ll` is byte-identical across runs.
- The final binary is byte-identical regardless of how many parallel jobs ran.

## Compiler performance

Ordered by expected gain on edits to a large project (doc 14 has the measurements behind the
first three):

1. [ ] **Per-module front-end reuse.** Skip parsing, analysis, HIR and MIR for modules whose
   `ModuleHashes::cache_key` is unchanged. This is the largest gain for edits.
2. [x] **Prelude cache.** Parsed prelude packages are reused for the process. Generator
   analysis across edits that leave generator inputs unchanged is still open (about 0.35 s
   with `@json`).
3. [ ] **Debug codegen units with an object cache** (above). An edit runs `llc` only for the
   modules it changed.
4. [ ] **Parallel compiler phases.**
   - MIR function passes run on a `rayon` pool. Function order is unchanged, and a MIR dump
     stays sequential so its trace does not depend on threads. Analysis stays sequential.
   - Debug LLVM text emission is still one module and one `llc`, until codegen units exist.
5. [ ] **Release `llvm-split` with parallel `llc`** (above).
6. [ ] **Prebuilt toolchain artifacts.** Ship runtime objects, runtime signatures and std
   generator executables with the toolchain. Cold Debug builds drop from 4–26 s to about the
   cost of an edit.

The Dream compiler itself:

- Stage 0 builds it with full Release optimization, and runtime performance close to .NET 10
  AOT carries over.
- The new types above (`OrderedMap`, `BitSet`, `Utf8Text`, `Utf8Builder`) remove the main
  avoidable costs: UTF-16 conversion, O(n) sorted inserts, and tree-based ID sets.
- Track compile time and peak memory with the `selfhost` benchmark at every phase. The Dream
  compiler should match or beat the Rust seed on that benchmark before the switch.

## Porting guide and decisions

- [ ] **`compiler/PORTING.md`.** Covers:
  - Mirrored file layout.
  - Same names, field order and control flow.
  - The `TODO(port)`, `PERF(port)` and `PORT NOTE` markers.
  - The Rust-to-Dream idiom map: enums and `match`, traits, `Option`/`Result`/`?`, borrows,
    collections, `format!`, iterator chains, panics, and unit tests as `@test`.
- [ ] **`compiler/OWNERSHIP.tsv`.** For every Rust field holding a borrow, a lifetime, an
  `Rc` or an `Arc`, record whether its Dream form is strong, weak, unowned or an integer ID.
  Strong cycles are rejected at compile time, so this has to be complete before Phase A.
- [ ] **ID newtypes.** Confirm that a single-field Dream `struct` (`TypeId`, `DefId`, `Local`,
  `BlockId`) is passed unboxed and can be a `Map` key or `BitSet` element at no cost.
- [ ] **Lazy statics.** Confirm a pattern for `OnceCell`, `OnceLock` and `LazyLock` (lazily
  initialized globals).
- [ ] **External crate replacements**, following the dependency policy:

| Rust crate | Used for | Replacement | Kind |
|---|---|---|---|
| `wasmprinter` | `.wat` next to every wasm build | Removed (see Stage 0 prerequisites) | removed |
| `oxc_*` | Parsing the JS runtime for tree shaking, host export analysis, minifying | `scripts/bundle-runtime.mjs` writes a requirements and host-export manifest at bundle time, embedded with `Assets`. Minify with a pinned `esbuild` | precomputed + tool |
| `object` | Reading capability ABI symbols from native libraries | Pinned `llvm-readobj` / `llvm-nm` | tool |
| `wasmparser` | Reading wasm sections and imports, components | Pinned `llvm-readobj` and `llvm-objcopy` (both handle wasm); otherwise a small section reader | tool |
| ABI custom section embedding | Writing the ABI section into `.wasm` | Pinned `llvm-objcopy --add-section` | tool |
| `brotli` | `.br` sidecars in release web builds | Pinned `brotli` tool, or drop `.br` and keep `.gz` | tool or removed |
| `flate2` | `.gz` sidecars | `system.compress` (deflate) | stdlib |
| `image` | Validating the `--icon` PNG | `system.drawing` image decoding, or a PNG header check | stdlib |
| `blake3` | Cache keys and artifact hashes | `system.crypto` SHA-256. Add BLAKE3 to the stdlib in Dream only if measurements require it | stdlib |
| `serde`, `serde_json`, `serde_stacker` | Generator models, ABI JSON, LSP protocol | `system.json`. Verify deep nesting without stack overflow | stdlib |
| `clap` | CLI | `system.cli` | stdlib |
| `tracing`, `tracing-subscriber` | Phase spans and logs | `system.logging` | stdlib |
| `rayon` | Parallel runtime-unit builds | `Task` | stdlib |
| `tempfile` | Temporary build directories | `system.io` temporary files | stdlib |
| `base64` | Generator assets | `system` encoding helpers | stdlib |
| `dunce` | Windows path normalization | `system.io` `Path` | stdlib |
| `filetime` | Cache budget timestamps | `system.io`; add setting a file's modification time (missing today) | stdlib gap |
| `half` | `float16` constants | `float16` primitive | stdlib |
| `indexmap`, `bumpalo` | Ordered tables, AST arenas | New `OrderedMap`/`OrderedSet`, classes then `Arena` | stdlib |
| `libc`, `windows-sys` | Process groups, job objects, file APIs | `system.process` / `system.io`; otherwise direct OS calls through Dream FFI (the OS, not a library) | stdlib |
| `logos` | Lexer | Hand-written lexer from the token table | compiler code |
| `toml` | `dream.toml` reads | Small reader for the subset `dream.toml` uses | compiler code |
| `depfile` | Clang `.d` dependency files | Small Make-syntax parser | compiler code |
| `which` | Tool lookup | Small path search | compiler code |
| `tower-lsp`, `tokio`, `dashmap` | LSP server | JSON-RPC over stdin/stdout with `Content-Length` framing, Dream `async`, hand-written types for the LSP subset used, `Map` with `Lock` (or one owner task) | stdlib + compiler code |

## Port phases

- [ ] **Phase A, drafts.** Write a `.dream` draft for every `.rs` file in scope, in a mirrored
  layout under `compiler/`. Adversarial review compares each pair side by side.
- [ ] **Phase B, compile.** Compile one module at a time in dependency order, checking off each:
  - [ ] text
  - [ ] diagnostics
  - [ ] syntax
  - [ ] format
  - [ ] types
  - [ ] hir
  - [ ] abi
  - [ ] stdlib registry
  - [ ] sema
  - [ ] mir lowering and verify
  - [ ] mir passes
  - [ ] mir LLVM backend
  - [ ] driver
  - [ ] execution
  - [ ] lsp
- [ ] **Phase C, parity.** Add `scripts/test_corpus.py --compiler <path>`.
  - [ ] AST, HIR and MIR dumps identical on every corpus case.
  - [ ] Diagnostics identical, including invalid cases and traps.
  - [ ] `.ll`, `.wasm` and `.abi.json` byte-identical, native and Node.
  - [ ] Parser fuzz inputs produce identical results from both compilers.
  - [ ] `cargo test -p dream-lsp` scenarios ported and passing against the Dream LSP; the VS
    Code extension works unchanged.
- [ ] **Phase D, fixed point.** Stage 0 builds Stage 1 (Release), Stage 1 builds Stage 2, and
  Stage 2 builds Stage 3.
  - [ ] Stage 2 and Stage 3 byte-identical.
  - [ ] Compile time, peak memory and hashes recorded against the Rust seed.
- [ ] **Phase E, switch.**
  - [ ] The Dream compiler becomes the default.
  - [ ] The Rust compiler crates are deleted.
  - [ ] `PERF(port)` cleanup.
  - [ ] Idiomatic passes.
  - [ ] `AGENTS.md` and `docs/internals/` updated for the new layout.

## Freeze

From Phase A until Phase D, do not merge language or compiler changes. Any change made in that
window has to be ported twice. Stage 0 bug fixes found during Phase B are the exception; each
fix lands in Rust with a corpus test and is mirrored in the draft.
