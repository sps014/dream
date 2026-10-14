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

Waits without tokens use scheduler completion wakeups. Pending token checks
currently use 1 ms timers; a submitted seek finishes before reporting cancellation.
Native filesystem calls already executing are not interrupted. The JS async bridge
pins host futures until Promise settlement and skips result allocation for
cancelled frames, preventing stale callbacks and discarded-result leaks.

`system.net` provides `Url` and ordered `UrlQuery`, using a separate native `net`
capability backed by `url`, plus host URL APIs on Node/browser. Parsing,
normalization, relative resolution, repeated keys, query mutation, and serialization
are implemented. The runtime chunk catalog supports multiple source files per
chunk so file-operation helpers are included only with filesystem imports.

Opaque-path space recovery differs between the current `url` crate and Node URL
implementation; shared fixtures use explicitly encoded spaces. Resolve this
conformance difference before claiming byte-identical serialization for arbitrary
non-HTTP schemes.

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

Token-triggered wakeups and full cross-platform/browser qualification remain
outstanding. HTTP operation completion currently uses 1 ms timer checks. Node
Fetch pools belong to the Node host and cannot be explicitly drained per Dream
client. These changes do not establish production v1 readiness.

## Remaining implementation

1. Phase 3: replace timer-based token checks with cancellation registrations and
   wakeups, qualify cancellation/closure races on all targets, and add platform
   nonblocking readiness where supported.
2. Phase 4: complete HTTPS success/redirect qualification across native platforms,
   broaden browser CORS/disconnect/cancellation automation, and qualify connection
   reuse/resource baselines under failure injection and stress.
3. Phase 5: declarative CLI parser, shared host conformance, help/version outcomes,
   and the downloadable-file CLI acceptance application.
4. Phase 6: bounded/unbounded channels, worker transfer, structured concurrency,
   process options, asynchronous pipes, deadlines, termination, and reaping.
5. Phase 7: CSV, UUID, temporary resources, atomic replacement, typed configuration,
   testing fixtures, async assertions, and the HTTP-to-CSV acceptance application.
6. Phase 8: cross-platform native and browser qualification, sanitizer/failure
   injection coverage, executable documentation, packaging, and release gates.
7. Phase 9: post-v1 TCP/UDP, streaming compression, application-driven cryptography,
   and collection refinements.

Keep guest runtime code in C, native capability services in host crates, and JS
adapters in the existing runtime source and bundling system. Follow the runtime
catalog, ABI registry, stdlib package registry, and strict workspace/corpus gates.

## Local verification

The foundation changes passed workspace build, strict workspace Clippy, workspace
tests, the full native/Node assertion corpus, and a final full Node rerun after the
BOM-preservation regression. The argument fixture also checks `System.args` on
both targets with empty, multiline, Japanese, and BOM-prefixed arguments.

The runtime layering check linked all 19 portable core units using the pinned
LLVM tools. API inventory checks, five inventory extractor tests, five JS host and
bundle tests, and all 19 executable documentation examples passed.

These results cover local macOS native execution and Node WebAssembly execution.
Browser automation and native Windows/Linux qualification remain release work.

## Phase 3–4 increment verification

The increment passes workspace build, strict workspace Clippy, workspace tests,
and the focused async-file/URL corpus on native and Node. Address/undefined-behavior
sanitizers pass both the native file-operation ownership harness and the native
Dream async-file fixture. Nine JS host/bundler tests, six API extractor tests,
and 22 executable documentation examples pass.

The browser smoke fixture at `tests/fixtures/compiler/stdlib_web_io_url.dream`
was compiled with `--runtime --web` and executed through browser automation. Its
URL normalization/query edits and async memory-filesystem reads/writes passed.
This is a local browser smoke check, not full browser or Windows/Linux qualification.

HTTP integration fixtures pass on native and Node, including streamed copies,
partial reads, repeated headers, HTTP error statuses, text limits, invalid UTF-8,
redirects, deadlines, pending cancellation, and closure. The native transport
fixture also checks trusted HTTPS, rejection of an untrusted certificate,
connection reuse, disconnects, and unclaimed-response cleanup. The native Dream
HTTP fixture passes address/undefined-behavior sanitizers.

The browser HTTP fixture at `tests/fixtures/compiler/stdlib_web_http.dream`
checks text reads, limits, pending cancellation, deadlines, truncated responses,
CORS rejection, and unsupported connection/redirect/header controls. Both browser
fixtures were compiled with `--runtime --web` and passed browser automation.

The complete native and Node assertion corpus passes with the HTTP increment,
including the regression for duplicate async-host declarations across stdlib modules.

The HTTP increment also passes workspace build, strict Clippy, and the complete
workspace test suite, including packaging checks. The TLS fixture selects its
Rustls provider explicitly because workspace feature unification enables multiple
providers. This verification remains local to macOS, Node, and the browser smoke fixtures.
