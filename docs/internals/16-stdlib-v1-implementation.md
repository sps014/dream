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

## Remaining implementation

1. Phase 3: cancellable pending-operation registration and wakeups, exactly-once
   completion, bounded blocking filesystem execution, nonblocking adapters, and
   safe late completion. Existing file async wrappers still perform synchronous
   operations; the new async stream interfaces do not change that behavior.
2. Phase 4: native networking capability, URL/HTTP public APIs, JS adapters,
   streaming responses, TLS, deadlines, redirects, cancellation, connection reuse,
   and explicit browser capability differences.
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
