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
publishes reports. Cross-platform execution results are pending for this increment.

## Remaining implementation

1. Phase 3–4: record the pending Linux/macOS/Windows qualification matrix.
2. Phase 5: declarative CLI parser, shared host conformance, help/version outcomes,
   and the downloadable-file CLI acceptance application.
3. Phase 6: bounded/unbounded channels, worker transfer, structured concurrency,
   process options, asynchronous pipes, deadlines, termination, and reaping.
4. Phase 7: CSV, UUID, temporary resources, atomic replacement, typed configuration,
   testing fixtures, async assertions, and the HTTP-to-CSV acceptance application.
5. Phase 8: cross-platform native and browser qualification, sanitizer/failure
   injection coverage, executable documentation, packaging, and release gates.
6. Phase 9: post-v1 TCP/UDP, streaming compression, application-driven cryptography,
   and collection refinements.

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
