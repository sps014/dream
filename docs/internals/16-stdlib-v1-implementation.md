# Stdlib v1 implementation status

The release target remains all P0 and P1 features on native, Node, and browser
hosts, subject to explicit host capabilities. This document distinguishes delivered
foundations from the remaining release work; it does not declare v1 readiness.

## Implemented foundations

Phase 1 adds error-bearing file reads, idempotent explicit closure, nonthrowing
handle destruction, closed-handle errors, handle-based length queries, and long
stream offsets. `FileStream` no longer caches an independently mutable position
or converts errors into empty data. Failed seeks preserve unread bytes.

Process launch arguments and `System.args` use counted, length-prefixed UTF-8
bytes. Native and Node decode argument boundaries without delimiters, validate
lengths, and reject NUL arguments before launch. Windows capture uses the OS Unicode
argument parser. Subprocess fixtures reject accidental recursive invocation if
argument decoding regresses. Bounded seeded randomness uses rejection sampling
over xorshift32's nonzero output domain.

Phase 2 adds `Reader`, `Writer`, `AsyncReader`, `AsyncWriter`,
`Streams.copy`, `Streams.copy_async`, `MemoryStream`, `BufferedReader`, and
`BufferedWriter`. Exact reads reject early EOF; complete writes retry partial
progress. Buffered writers retain unwritten bytes after failed flushes and require
explicit finishing. File line reading uses read-ahead with coherent logical
positions; wrapper seeking invalidates buffers only after success.

`Utf8Decoder` carries incomplete sequences across chunks, supports strict and
replacement policies, and validates finalization. URI-component and form codecs
have separate `+` handling and validate decoded UTF-8.

Usage and public declarations are documented in the [stream guide](../reference/stdlib/file-streams.md),
[encoding guide](../reference/stdlib/encoding.md), and API lookup pages. Shared
Dream suites exercise these foundations independently on native and Node. Direct
JS adapter tests exercise Node files and the in-memory filesystem. These do not
replace browser automation or qualification on Windows and Linux.

## Phase 3–4 progress

File opens, reads, writes, and seeks now run on a bounded C executor natively
(four workers, 128 queued jobs) and callback-based Node filesystem adapters.
Operations own copied arguments, write buffers, and descriptor references until
completion. Abandoned open results are closed. Files implement the async stream
interfaces; overlapping operations and closure while pending return `EBUSY`.
Read-ahead remains coherent across async reads and seeks.

Completion and cancellation use scheduler wakeups through detachable event
registrations. No periodic checks are needed; a submitted seek finishes before
reporting cancellation.
Native filesystem calls already executing are not interrupted. The JS async bridge
pins host futures until Promise settlement and skips result allocation for
cancelled frames, preventing stale callbacks and discarded-result leaks.

`system.net` provides `Url` and ordered `UrlQuery`, using a separate native `net`
capability backed by `url`, plus host URL APIs on Node/browser. Parsing,
normalization, relative resolution, repeated keys, query mutation, and serialization
are implemented. The runtime chunk catalog supports multiple source files per
chunk so file-operation helpers are included only with filesystem imports.

Opaque paths now encode a final space before query/fragment delimiters uniformly
in native and JavaScript adapters. Shared fixtures cover literal and encoded spaces.

HTTP transport now uses reqwest/Tokio in the native `net` capability and Fetch
in the shared Node/browser adapter. Clients, requests, case-insensitive repeated
headers, options, response metadata, streaming bodies, bounded strict UTF-8 reads,
and typed errors are available. Closing clients cancels pending sends; bodies own
their transports independently. Response metadata remains owned by its operation
until Dream explicitly claims it, including when an awaiting parent is abandoned.

Requests support deadlines through body consumption, configured native connection
timeouts, native/Node redirect limits, cancellation, and connection pooling.
Browser capability queries expose unavailable controls, and requests reject
restricted headers or unsupported options. HTTP status errors remain transport
successes. Native TLS uses normal certificate validation; no bypass is exposed.

Cancellation registrations live in the shared C runtime and own synchronously
registered signal futures. Native cancellation publishes a foreign completion to
the owning scheduler; same-owner detachment completes inline. JavaScript uses
local notifications or `Atomics.waitAsync` for cross-worker shared memory. Closing
a subscription settles abandoned callbacks before its storage can be reused.
`CancellationToken.wait`, `wait_void`, and `wait_cancelled` expose these waits publicly.

HTTP completion now has its own notification from Tokio/Fetch instead of a timer
loop. Node Fetch pools still belong to the Node host and cannot be explicitly
drained per Dream client. These changes do not establish production v1 readiness.

The phase 3–4 qualification command (`scripts/qualify-io-net.py`) runs native and
Node fixtures in debug/release modes, transport/TLS tests, and adapter/ownership
checks. The dedicated platform workflow runs it on Linux, macOS, and Windows and
publishes reports. The [Linux/macOS/Windows run](https://github.com/sps014/dream/actions/runs/38684962085)
passes all 13 checks on each platform at `e34ce58c`. Each archived report identifies
the revision and confirms a clean source tree.

## Phase 5 implementation

`system.cli` provides declarative commands, boolean flags, typed string/integer
options, required and repeated positionals, defaults, choices, subcommands, and
structured help/version outcomes. Repeated options preserve occurrence order;
typed single-value getters return the last occurrence. Explicit-value provenance
is kept separately from defaults.

The `cli` native capability and JavaScript adapter share one clap implementation.
The adapter embeds a separately compiled Rust WebAssembly capability; it does not
add Rust to the Dream guest runtime. It imports no host functions and is instantiated
only when CLI parsing is used. `scripts/build-cli-parser.py` rebuilds it with the
`wasm32-unknown-unknown` target; `--check` verifies its source/dependency fingerprint.
The existing JS bundler includes this capability only for live CLI imports.

Shared fixtures cover defaults, short clusters, negative numbers, overflow,
repetition, `--`, empty/Unicode/multiline arguments, subcommands, generated help,
version, and invalid definitions. The complete `sample/cli_download` application
streams HTTP responses to files, reports exit statuses, and explicitly closes its
resources. Native/Node integration checks downloads, help/version, bad arguments,
timeouts, and preserving destinations on HTTP error statuses.

## Phase 6 implementation

`Task.scope` owns eager local async children, joins discarded handles, propagates
cancellation, and retains typed application errors. Deadlines request cancellation;
CPU work remains cooperative. Scope futures stay schedulable through cleanup even
when explicitly cancelled. Child completion signals support independent joiners.
Async I/O, timers, channels, processes, and pool jobs inherit the current scope token.
Explicit low-level promise/worker operations retain their explicit token contracts.

`Task.map` and `Task.for_each` use bounded local worker loops, preserve input order,
and stop taking inputs on failure. Reusable `TaskPool` jobs provide explicit CPU
parallelism, and `scope.spawn_on` joins pool work inside a region. Native worker
receives and pooled replies now use foreign completion wakeups instead of blocking
an async poll. Submitted callback environments stay retained through completion.

Local and shared channels provide bounded/unbounded factories, separate endpoints,
partial closure, drained EOF, and cancellation of blocked operations. Readiness
wakes one FIFO waiter; closure wakes all. Shared channels enforce shared payload
constraints, publish typed carriers, and synchronize their C queues across workers.
Cancellation state reads and reference counts use atomics where threads are enabled.

Process options snapshot environment overrides, working directories, input bytes,
output limits, and deadlines. Native services use a bounded Tokio runtime; Node
uses asynchronous child-process streams. Both drain stdout/stderr concurrently,
propagate pipe failures, and terminate/reap owned run children before returning
cancellation or timeout. Borrowed process pipes implement the async stream APIs.
Browser launch requests return EUNSUPPORTED. Unix process I/O workers block SIGPIPE
to report broken pipes as errors; child exec resets that mask.

Public fields and methods have source comments and API reference coverage. The
source inventory now recognizes shared public classes. Generic method bodies keep
owner substitutions, generic calls support contextual lambdas/named arguments,
and monomorphization bounds use semantic nesting rather than rendered symbol length.
A runnable bounded pipeline and concurrency benchmark accompany the APIs.

## Phase 7 implementation

`system.csv` provides strict UTF-8 streaming records with quoted fields, embedded
newlines, configurable ASCII delimiters, record limits, and byte-oriented errors.
A shared Rust `csv` adapter supplies field decoding and writer escaping on native
and WASM hosts. `system.uuid` stores 128 bits inline as two `ulong`s, with value
equality, hashing, ordering, secure v4 generation, and canonical formatting.
Secure random APIs now return errors and discard partial entropy on host failure;
AES key/nonce factories and callers use the fallible APIs directly.

`TempFile` and `TempDirectory` use private OS directories through `tempfile` on
native and `mkdtemp` on Node. Explicit cleanup reports errors; destructor cleanup
is best-effort. Atomic replacement flushes and closes output first, preserves the old
destination on rename failure, and exposes unsupported browser capabilities.
The optional `io` host owns temporary OS services, leaving core binding state lean.

Typed configuration merges defaults, JSON, environment, and explicit CLI values
with validation and source-aware diagnostics. Async assertions and typed fixture
owners extend the existing async runner. The HTTP-to-CSV example spools downloads
with bounded memory, adds UUIDs, and atomically publishes validated output.

## Phase 8 qualification infrastructure

`scripts/qualify-stdlib-v1.py` runs release gates sequentially and records the
revision, dirty state, platform, step results, and logs. Its native/Node gate
includes workspace builds and tests, strict Clippy, both guest profiles, executable
docs, full-corpus determinism, packaging, and sanitizer checks. The separate
browser harness checks debug/release fixtures and unsupported capabilities.
`.github/workflows/stdlib-v1.yml` runs native qualification on Linux, macOS, and
Windows and browser qualification on Linux, preserving evidence as artifacts.
A gate is not qualified until its corresponding report passes.

The application example exposed excessive Wasm locals in large debug async polls.
Debug Wasm program code generation now enables llc stackification/coalescing;
higher-level debug MIR/LLVM optimization policy is unchanged. Invalid WAT
rendering returns a toolchain diagnostic, and quiet panic output is confined to
the codegen catch region.

## Phase 9 implementation

Raw TCP and UDP use Tokio native services and asynchronous Node adapters. TCP
implements async streams, partial writes, deadlines, cancellation, half-close,
TCP_NODELAY, and independent accepted-stream ownership. UDP preserves datagram
boundaries, empty messages, explicit truncation, broadcast and TTL options.
Close wakes pending operations. Browsers expose unsupported raw-socket capability.

Streaming gzip/zlib wrappers share `flate2`/`zlib-rs` on native and WASM adapters,
with chunk/output limits, checksum/truncation errors, explicit finish, and strict
single-stream termination. List, Queue, Map, and Set share additional-element
reserve semantics and seeded operation-sequence tests. Additional cryptography
remains conditional on concrete application requirements; no speculative
algorithms are introduced.

## Remaining qualification

Complete the Phase 8 native platform matrix, browser matrix, packaging,
determinism, and sanitizer gates before declaring a stable v1. Record actual
reports separately from implementation status.

Keep guest runtime code in C, native capability services in host crates, and JS
adapters in the existing runtime source and bundling system. Follow the runtime
catalog, ABI registry, stdlib package registry, and strict workspace/corpus gates.

## Verification

The event-driven increment passes local macOS workspace build, strict workspace
Clippy, the complete workspace test suite, and the full native and Node assertion
corpus. Qualification also exposed a nested Rayon/output-lock deadlock and Apple
Clang SDK metadata being excluded from cache dependencies; both have fixes and
passing regression coverage.

The cancellation C harness races 120,000 registrations against cancellation and
checks one completion per signal and safe detachment. It passes address/undefined
and thread sanitizers on macOS, plus plain and address/undefined builds on Linux
ARM64. Native Dream ownership probes cover waits returned from local token scopes,
heap-backed results, and void futures without leaked resources.

Browser automation passes four fixtures in debug and release modes: asynchronous
file/URL operations, HTTP, local cancellation, and cross-worker cancellation. The
HTTP fixture checks bounded text reads, pending cancellation, deadlines, truncated
responses, CORS rejection, and unsupported connection/redirect/header controls.
Run `python scripts/serve-phase34-browser.py` (or add `--release`) to compile and
serve these fixtures again.

Native/Node HTTP fixtures cover streaming, short reads, repeated headers, error
statuses, text limits, strict UTF-8, redirects, deadlines, cancellation, and closure.
Native transport fixtures additionally cover trusted HTTPS, certificate rejection,
connection reuse, disconnects, and unclaimed-response cleanup. Direct JavaScript
adapter/bundler checks cover cancellation subscriptions, shared-memory growth,
HTTP completion/release, and host selection.

The platform workflow preserves `target/phase34-qualification/report.json` and
per-step logs. The broader phase 8 release gate still includes the complete
packaging, determinism, failure-injection, and supported-platform matrix.

Phase 5 passes local workspace build, strict Clippy, workspace tests, the complete
native/Node corpus, and the CLI corpus in guest release mode. Thirty-six JavaScript
host/bundler tests, six documentation extractor tests, all 25 executable guide
examples, and the strict documentation build pass. All 29 new public declarations
have source comments and API reference entries. The download integration asserts
zero live guest resources after success, help/version, validation errors, deadlines,
and HTTP failures on both native and Node.

The CLI browser fixture passes in debug and release modes with explicit arguments,
Unicode values, short clusters, negative integers, `--`, help/version, and invalid
definitions. Run `python scripts/serve-phase34-browser.py --case cli` to repeat it.
The full native platform matrix for Phase 3–4 remains separately recorded above;
Phase 8 still qualifies the completed v1 across platforms and packaging modes.

Phase 6 passes the full native and Node assertion corpus, plus optimized native
and Node checks for structured scopes, channels, and process operations. Browser
automation passes structured concurrency and shared-worker channels in debug and
release builds, checking that no guest resources remain live after completion.
The bounded pipeline sample returns 5050 without guest leaks on native and Node.

The phase also passes strict workspace Clippy, 44 JavaScript adapter/bundler tests,
seven documentation extractor tests, all 28 executable documentation examples,
and the strict documentation build. Cancellation registration stress passes
address/undefined and thread sanitizers on macOS. These Phase 6 results qualify
the local macOS host; Linux and Windows qualification remains part of Phase 8.
