# Audio implementation status

`system.audio` currently provides streamed file/HTTP playback, memory-backed
playback, automatic WAV/MP3 and
WebM/Opus decoding, supplied-PCM playback, offline FFT analysis, typed polling
events, output selection, and rendered-output capture. The public usage guide is
[system.audio](../library/system/audio.md).

## Runtime boundaries

The guest FFT and decoder units are optional C modules in the runtime catalog.
The native audio capability owns miniaudio output engines and shares an engine
between players with the same selected output. Its control lock is never acquired
by an audio callback. A lazily attached node copies post-gain stereo samples into
an atomic bounded ring. A sequence check makes concurrent snapshots explicit
without blocking the callback.

Browser output shares an AudioContext per selected sink. Buffer sources use
AudioBufferSourceNode followed by gain and worklet output; streams use a
`dream-audio-stream` worklet with a frame ring, linear-interpolation resampling,
and epoch-tagged flushes. The main thread keeps unplayed chunks so a sink switch
can re-post them to a replacement node. Capture is an additional
selectively emitted chunk, enabled on first capture, with eight transferable PCM
buffers and a 32,768-frame history. Neither output nor capture requires shared
memory. Closing and source replacement invalidate pending asynchronous starts.

## Streaming

Browser HTTP URLs use `HTMLAudioElement` through the separately selected
`audio_media` host chunk. The browser owns decoding, network buffering, container
indexes, and HTTP range requests. Each URL player has a private AudioContext so
changing its sink cannot reroute peers; gain and capture remain in its audio graph.
Native and Node return zero from media creation and use the portable stream path.

Other string sources open an `AudioStreamCursor` (AsyncReader over a file or HTTP body)
feeding `core/audio_stream.c`, an incremental decoder over a bounded compressed
queue. The vendored dr_wav, dr_mp3, and Nestegg parsers expect random access, so
the queue enforces a lookahead invariant instead of a custom parser: a decoder is
entered only when the queue holds its lookahead (256 KiB before initialization,
128 KiB for WAV and MP3, 2 MiB for WebM), input has ended, or the queue is
full. After initialization at most min(64 KiB, capacity / 4) is kept behind the
read cursor. A read that starves while input remains reports `ELIMIT`, which is
how undecodable-within-limit sources fail. ID3v2 tags are skipped as they arrive.

`AudioStreaming.pump` moves 4,096-frame chunks into a host ring sized by
`buffer_seconds`. Exactly one task owns the feed (`feed_owned`): the pump while it
runs, otherwise a seek task, so a chunk decoded before a flush can never be written
after it. Seeks during playback are applied by the pump between chunks. Local
seeks reopen the source and decode-skip. Remote PCM/float WAV seeks get a validated
byte offset from dr_wav's data chunk and block alignment and send Range with
If-Range. Response status, resource validator, start position, and total length
are checked before replacing the reader or rebasing the decoder. Unsupported
remote formats return ENOTSUP without decode-skipping. Loops record the wrap frame
so `current_time` stays continuous with the ring's absolute frame counter.

The native host's stream source is a miniaudio data source over a single-producer
ring of absolute frame counters. An underrun emits silence without advancing the
read index and raises a starved flag that `synchronize()` turns into
`Waiting`/`Playing`. Control 3 flushes and rebases, 6 marks end of stream.

## Off-thread decoding

Codec code never runs on the guest scheduler. The raw codec API in
`core/include/dream_audio.h` (`dream_audio_decode_words` and the
`dream_audio_stream_*` decoder) touches only its inputs and the platform
allocator, so any thread may call it. Dream reaches it through `@runtime`
`audioDecode*` hosts that the `audio_decode` catalog module implements per
environment:

- Native, `sys/native/audio_jobs.c` (a `native_runtime_c` unit, so it is
  optimized with the runtime bitcode): a one-shot decode copies its input and runs
  on the blocking pool, following the `FileOperation` shape of start, wait,
  result, and release with a reference held by the pool. Each stream owns one
  serial job. Encoded input queues beside it, and space, duration, and the WAV
  seek layout are snapshots that every job refreshes, so `push`, `space`,
  `duration`, and `seek_offset` stay synchronous. After a chunk is claimed, the
  next one is decoded ahead. A read that returns "need input" is re-run before
  publishing when input or end of input arrived meanwhile. A reposition discards
  the in-flight result and queued input. Dream arrays are built only on the
  scheduler thread.
- Browser and Node, `runtime/src/hosts/audio_decoder.js` (the `audio_decoder`
  chunk, wired in `load.js`): two lazily started workers, one for one-shot
  decodes and one for all streams, so a long file decode never delays a playing
  stream. Each worker instantiates the program's compiled module again with a
  private memory of the same shape, stub imports that throw, and only
  `dream_heap_init` plus TLS setup. It never runs `__runtime_init` or the program.
  It calls the `dream_audio_worker_*` exports from `sys/wasi/audio_worker.c`,
  which `link_wasm` keeps exported through the module's `worker_exports`. Input
  and PCM cross as transferred buffers, so no `SharedArrayBuffer` or COOP/COEP is
  needed. The main thread mirrors the native stream state machine.
- WASI components, `sys/wasi/component/audio_jobs.c`: components have no
  threads, so the same hosts decode inline and complete the wait immediately.

The stdlib side is `AudioDecodeJob` (one-shot) and `AudioStreamDecoder`
(streams). `AudioBuffer.decode`, `AudioStreamDecoder.read`, and
`reposition` are async; a cancelled wait returns `ECANCELLED` while the
job finishes unobserved, and a later wait completes any earlier abandoned
waiter so its scheduler credit is returned. FFT stays inline because one
transform costs microseconds.

Streaming operations are reachable only through closures installed by string-source
constructors, so PCM-only programs never reference the stream decoder. Byte
constructors snapshot their input and decode completely. FFT-only programs retain
no output, networking, or decoder units; PCM constructors do not reference decoders.

Optimized native deployments and WASM have byte-identity fixtures for unused and
unreachable audio. Native PE timestamps use reproducible content IDs. Native
relocatable builds record owned companions per executable and remove obsolete
libraries only after the last sibling manifest releases them.

## Outstanding work from the approved plan

- Native remote MP3 and WebM index seeking remains unsupported; HTTP seeking is
  limited to independently decodable PCM and float WAV. A second pending seek
  reports EBUSY.
- Streamed MP3 duration. dr_mp3 needs the whole file to count frames, so streamed
  MP3 sources report no duration.
- The approved plan named libwebm. The current WebM parser is pinned Nestegg,
  which is portable C; replacing it with the requested C++ parser behind the
  capability C ABI remains a deviation from that plan.
- Native debug artifacts retain otherwise unused type metadata, so their
  byte-identity deployment guarantee is not yet met.
- Native capture is lazily allocated, but its exported host implementation is in
  the same capability library as playback. Per-symbol native capture packaging
  remains to be separated if the unused tap itself must be absent from that DLL.
- Hardware device removal, permissions, and switching need broader qualification
  on macOS, Linux, and additional browsers. Native opaque device IDs currently
  include the enumeration index and name rather than persistent hardware identity.
- WASI Preview 2 WAV decoding and FFT have been qualified with Wasmtime. Preview 3
  and broader standalone-WASI format fixtures remain to be checked.

## Focused checks

`tests/projects/core/audio_analysis.dream` covers PCM ownership and bounds,
content detection, WebM/Opus tone analysis, FFT normalization, control errors,
closure, ordered event overflow, a large decode that must leave a timer turn
free, and a cancelled decode. `optimized_wasm_audio_decoder_exports_worker_entry_points`
checks that optimized WASM keeps the decode-worker exports. `scripts/test-audio-host.mjs` covers output
sharing, failed sink preservation, cancellation during context startup, and
browser picker error distinctions. `scripts/test-audio-browser.py` runs a real
Chromium decode-worker, playback, and capture check, plus streamed HTTP progress,
capture, and seek. `scripts/test-audio-trimming.py --native`
compares deployment bytes and verifies codec, FFT, capture, and output exclusion,
including rebuilding a native deployment without audio.

`scripts/test-audio-sources.py` serves deliberately mislabeled audio in tiny HTTP
writes and checks detection, limits, malformed data, transport errors, replacement,
and cancellation during loading on native and Node. It also streams a long WAV
and an ID3-tagged MP3 through a 256 KiB queue; natively it plays them to check
progress, forward and backward seeks, loop wrapping, end and replay, and an HTTP
stall that must produce `Waiting` then `Playing`. The `audio_offline.dream`
fixture checks WASI decoding and analysis without importing output capabilities.

The native/browser visualizer lives in `sample/audio_visualizer` and uses the
public audio, window, and GPU APIs.
