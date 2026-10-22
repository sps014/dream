import { isNode } from "../platform.js";

// The decode worker instantiates the program's own module with private memory and calls only
// the exported `dream_audio_worker_*` codec entry points, so it never runs the program, never
// shares memory with the page, and needs no cross-origin isolation. Every import is a stub that
// throws, because a codec path that reaches a host call is a runtime bug.
const DECODE_WORKER_BODY = `
let ex = null;
let memory = null;
const decoders = new Map();
const stub = (name) => () => {
  throw new Error("the audio decode worker cannot call " + name);
};
async function boot(module, descriptor) {
  memory = new WebAssembly.Memory(descriptor);
  const imports = {};
  for (const entry of WebAssembly.Module.imports(module)) {
    const bucket = (imports[entry.module] ||= {});
    if (entry.kind === "memory") bucket[entry.name] = memory;
    else if (entry.kind === "function") bucket[entry.name] = stub(entry.module + "." + entry.name);
  }
  ex = (await WebAssembly.instantiate(module, imports)).exports;
  ex.dream_heap_init();
  const tlsSize = ex.__tls_size ? ex.__tls_size.value >>> 0 : 0;
  if (tlsSize) {
    const align = (ex.__tls_align ? ex.__tls_align.value >>> 0 : 0) || 1;
    const block = ex.dream_wasm_raw_malloc(tlsSize + align - 1) >>> 0;
    ex.__wasm_init_tls(Math.ceil(block / align) * align);
  }
}
function copyIn(bytes) {
  if (!bytes.length) return 0;
  const pointer = ex.dream_audio_worker_alloc(bytes.length) >>> 0;
  if (pointer) new Uint8Array(memory.buffer, pointer, bytes.length).set(bytes);
  return pointer;
}
function report(pointer) {
  const view = new DataView(memory.buffer, pointer >>> 0, 56);
  return {
    status: view.getInt32(0, true),
    rate: view.getInt32(4, true),
    channels: view.getInt32(8, true),
    space: view.getInt32(12, true),
    samples: view.getUint32(16, true),
    count: view.getUint32(20, true),
    duration: view.getFloat64(24, true),
    dataPosition: view.getBigUint64(32, true),
    blockAlign: view.getBigUint64(40, true),
    frames: view.getBigUint64(48, true),
  };
}
function floats(pointer, count) {
  return count ? new Float32Array(memory.buffer, pointer, count).slice() : new Float32Array(0);
}
function decode(m) {
  const input = copyIn(m.bytes);
  if (m.bytes.length && !input) return { id: m.id, words: Float32Array.of(-2) };
  const r = report(ex.dream_audio_worker_decode(input, m.bytes.length, m.max));
  if (input) ex.dream_audio_worker_free(input);
  const words = floats(r.samples, r.count);
  if (r.samples) ex.dream_audio_worker_free(r.samples);
  return { id: m.id, words };
}
function streamReply(id, r, samples) {
  return {
    id,
    status: r.status,
    rate: r.rate,
    channels: r.channels,
    samples,
    space: r.space,
    duration: r.duration,
    dataPosition: r.dataPosition,
    blockAlign: r.blockAlign,
    frames: r.frames,
  };
}
function handle(m) {
  if (m.t === "decode") return decode(m);
  if (m.t === "open") {
    decoders.set(m.key, ex.dream_audio_worker_stream_open(m.capacity) >>> 0);
    return null;
  }
  const decoder = decoders.get(m.key) || 0;
  if (m.t === "close") {
    decoders.delete(m.key);
    if (decoder) ex.dream_audio_worker_stream_close(decoder);
    return null;
  }
  if (m.t === "reposition") {
    return streamReply(m.id, report(ex.dream_audio_worker_stream_reposition(decoder, m.frame)),
                       new Float32Array(0));
  }
  const input = copyIn(m.bytes);
  if (m.bytes.length && !input) {
    return { id: m.id, status: -2, rate: 0, channels: 0, samples: new Float32Array(0), space: 0,
             duration: -1, dataPosition: 0n, blockAlign: 0n, frames: 0n };
  }
  const r = report(ex.dream_audio_worker_stream_read(decoder, input, m.bytes.length,
                                                     m.finish ? 1 : 0, m.maxFrames));
  if (input) ex.dream_audio_worker_free(input);
  return streamReply(m.id, r, floats(r.samples, r.count));
}
let ready = null;
function receive(m) {
  if (m.t === "init") {
    ready = boot(m.module, m.memory);
    ready.catch((err) => post({ t: "fail", message: String((err && err.stack) || err) }));
    return;
  }
  ready.then(() => {
    const reply = handle(m);
    if (reply) post(reply, reply.words ? [reply.words.buffer] : [reply.samples.buffer]);
  }).catch((err) => post({ t: "fail", message: String((err && err.stack) || err) }));
}
`;

const NODE_DIALECT = `const { parentPort } = require("node:worker_threads");
const post = (message, transfer) => parentPort.postMessage(message, transfer);
parentPort.on("message", receive);
`;

const BROWSER_DIALECT = `const post = (message, transfer) => self.postMessage(message, transfer);
self.onmessage = (event) => receive(event.data);
`;

const FAILED_WORDS = () => Float32Array.of(-2);

/**
 * One lazily started decode worker. Messages are handled in order, so a stream's open, reads
 * and close never race. A failed worker answers every outstanding and later call with an
 * out-of-memory status instead of leaving a Dream future pending forever.
 */
function makeDecodeWorker(wasmModule, memoryDescriptor) {
  let worker = null;
  let starting = false;
  let failed = false;
  let nextId = 1;
  const queued = [];
  const pending = new Map();

  const keepAlive = () => {
    if (!worker || !isNode) return;
    if (pending.size) worker.ref();
    else worker.unref();
  };

  const fail = () => {
    failed = true;
    queued.length = 0;
    const owed = [...pending.values()];
    pending.clear();
    for (const settle of owed) settle(null);
    if (worker) {
      try {
        worker.terminate();
      } catch (_) {
        /* already stopped */
      }
    }
  };

  const receive = (m) => {
    if (m.t === "fail") {
      fail();
      return;
    }
    const settle = pending.get(m.id);
    pending.delete(m.id);
    keepAlive();
    if (settle) settle(m);
  };

  const attach = (created) => {
    worker = created;
    if (isNode) {
      worker.on("message", receive);
      worker.on("error", fail);
      worker.on("exit", () => {
        if (pending.size) fail();
      });
    } else {
      worker.onmessage = (event) => receive(event.data);
      worker.onerror = (event) => {
        if (typeof event.preventDefault === "function") event.preventDefault();
        fail();
      };
    }
    worker.postMessage({ t: "init", module: wasmModule, memory: memoryDescriptor });
    for (const [message, transfer] of queued) worker.postMessage(message, transfer);
    queued.length = 0;
    keepAlive();
  };

  const start = () => {
    starting = true;
    if (isNode) {
      import("node:worker_threads")
        .then(({ Worker }) => attach(new Worker(DECODE_WORKER_BODY + NODE_DIALECT, { eval: true })))
        .catch(fail);
    } else if (typeof Worker !== "undefined") {
      // The URL stays valid for the page's lifetime; revoking it early can abort the worker's load.
      const url = URL.createObjectURL(
        new Blob([DECODE_WORKER_BODY + BROWSER_DIALECT], { type: "text/javascript" }),
      );
      try {
        attach(new Worker(url));
      } catch (_) {
        URL.revokeObjectURL(url);
        fail();
      }
    } else {
      failed = true;
    }
  };

  const send = (message, transfer = []) => {
    if (failed) return false;
    if (!starting) start();
    if (failed) return false;
    if (worker) worker.postMessage(message, transfer);
    else queued.push([message, transfer]);
    return true;
  };

  return {
    /** Posts a request and resolves with the worker's reply, or null when the worker failed. */
    call(message, transfer) {
      const id = nextId++;
      return new Promise((resolve) => {
        if (!send({ ...message, id }, transfer)) {
          resolve(null);
          return;
        }
        pending.set(id, resolve);
        keepAlive();
      });
    },
    send,
    get available() {
      return !failed && (isNode || typeof Worker !== "undefined");
    },
  };
}

function ownedBytes(bytes) {
  if (bytes instanceof Uint8Array) return bytes.slice();
  return Uint8Array.from(bytes ?? []);
}

function joinInput(chunks, length) {
  if (chunks.length === 1) return chunks[0];
  const joined = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    joined.set(chunk, offset);
    offset += chunk.length;
  }
  return joined;
}

const NONE = 0;
const READ = 1;
const REPOSITION = 2;
const INT64_MAX = 0x7fffffffffffffffn;

/**
 * Hosts behind `AudioBuffer.decode` and the streaming decoder. One-shot decodes and streams use
 * separate workers so a long file decode never delays the next chunk of a playing stream.
 *
 * Each stream mirrors the native job state machine: encoded input queues beside the single
 * in-flight request, space, duration and seek layout are snapshots each reply refreshes, and
 * after a chunk is claimed the next one is decoded ahead.
 */
export function makeAudioDecoderModule(wasmModule, memoryDescriptor) {
  const oneShot = makeDecodeWorker(wasmModule, memoryDescriptor);
  const streaming = makeDecodeWorker(wasmModule, memoryDescriptor);
  const jobs = new Map();
  const streams = new Map();
  let nextHandle = 1;

  const settle = (s) => {
    const waiter = s.waiter;
    s.waiter = null;
    if (waiter) waiter(0);
  };

  const publish = (s, kind) => {
    s.ready = true;
    s.readyKind = kind;
    if (s.operation === kind) settle(s);
  };

  const run = async (s) => {
    for (;;) {
      if (s.closed || s.request === NONE) {
        s.running = false;
        settle(s);
        break;
      }
      const kind = (s.current = s.request);
      const finish = s.finishPending;
      s.request = NONE;
      s.busy = true;
      s.discard = false;
      s.finishPending = false;
      const input = joinInput(s.input, s.inputLength);
      s.input = [];
      s.inputLength = 0;
      const reply = kind === REPOSITION
        ? await streaming.call({ t: "reposition", key: s.key, frame: s.frame })
        : await streaming.call(
            { t: "read", key: s.key, bytes: input, finish, maxFrames: s.maxFrames },
            [input.buffer],
          );
      const status = reply ? reply.status : -2;
      if (kind === READ && status === 0 && !s.discard && s.request === NONE &&
          (s.inputLength || s.finishPending)) {
        s.request = READ;
        continue;
      }
      s.busy = false;
      if (reply) {
        s.space = reply.space;
        s.duration = reply.duration;
        s.layout = reply;
      } else {
        s.space = 0;
      }
      if (!s.discard) {
        if (kind === REPOSITION) {
          s.status = reply ? reply.status : 0;
          s.samples = new Float32Array(0);
        } else {
          s.status = status;
          s.rate = reply ? reply.rate : 0;
          s.channels = reply ? reply.channels : 0;
          s.samples = reply && status > 0 ? reply.samples : new Float32Array(0);
        }
        publish(s, kind);
      }
    }
    if (s.closed) streaming.send({ t: "close", key: s.key });
  };

  const schedule = (s) => {
    if (s.running) return 1;
    if (!streaming.available) {
      s.request = NONE;
      return 0;
    }
    s.running = true;
    run(s);
    return 1;
  };

  return {
    audioDecodeStart(bytes, maxBytes) {
      if (!oneShot.available) return 0;
      const input = ownedBytes(bytes);
      const job = { done: false, words: null, waiter: null };
      const handle = nextHandle++;
      jobs.set(handle, job);
      oneShot.call({ t: "decode", bytes: input, max: maxBytes }, [input.buffer]).then((reply) => {
        job.words = reply ? reply.words : FAILED_WORDS();
        job.done = true;
        const waiter = job.waiter;
        job.waiter = null;
        if (waiter) waiter(0);
      });
      return handle;
    },

    audioDecodeWait(handle) {
      const job = jobs.get(handle);
      if (!job || job.done) return Promise.resolve(0);
      return new Promise((resolve) => {
        const previous = job.waiter;
        job.waiter = resolve;
        if (previous) previous(0);
      });
    },

    audioDecodeResult(handle) {
      const job = jobs.get(handle);
      return job && job.done ? job.words : new Float32Array(0);
    },

    audioDecodeRelease(handle) {
      jobs.delete(handle);
    },

    audioDecodeStreamOpen(capacity) {
      if (capacity <= 0 || !streaming.available) return 0;
      const handle = nextHandle++;
      const s = {
        key: handle,
        closed: false,
        running: false,
        busy: false,
        request: NONE,
        current: NONE,
        discard: false,
        finishPending: false,
        finished: false,
        maxFrames: 0,
        space: capacity,
        frame: 0n,
        input: [],
        inputLength: 0,
        duration: -1,
        layout: null,
        operation: NONE,
        ready: false,
        readyKind: NONE,
        status: 0,
        rate: 0,
        channels: 0,
        samples: new Float32Array(0),
        waiter: null,
      };
      if (!streaming.send({ t: "open", key: handle, capacity })) return 0;
      streams.set(handle, s);
      return handle;
    },

    audioDecodeStreamSpace(handle) {
      const s = streams.get(handle);
      if (!s || s.finished) return 0;
      return Math.max(0, s.space - s.inputLength);
    },

    audioDecodeStreamPush(handle, bytes) {
      const s = streams.get(handle);
      const length = bytes ? bytes.length : 0;
      if (!s || s.finished || length > s.space - s.inputLength) return 0;
      if (length) {
        s.input.push(ownedBytes(bytes));
        s.inputLength += length;
      }
      return 1;
    },

    audioDecodeStreamFinish(handle) {
      const s = streams.get(handle);
      if (!s) return;
      s.finished = true;
      s.finishPending = true;
    },

    audioDecodeStreamRead(handle, maxFrames) {
      const s = streams.get(handle);
      if (!s) return 0;
      s.operation = READ;
      const pending = s.request === READ || (s.busy && s.current === READ && !s.discard);
      if ((s.ready && s.readyKind === READ) || pending) return 1;
      s.ready = false;
      s.request = READ;
      s.maxFrames = maxFrames;
      return schedule(s);
    },

    audioDecodeStreamReposition(handle, frame) {
      const s = streams.get(handle);
      if (!s) return 0;
      s.operation = REPOSITION;
      s.ready = false;
      s.discard = s.running;
      s.input = [];
      s.inputLength = 0;
      s.finished = false;
      s.finishPending = false;
      s.request = REPOSITION;
      s.frame = BigInt(frame);
      return schedule(s);
    },

    audioDecodeStreamWait(handle) {
      const s = streams.get(handle);
      if (!s || (s.ready && s.readyKind === s.operation) || !s.running) return Promise.resolve(0);
      return new Promise((resolve) => {
        const previous = s.waiter;
        s.waiter = resolve;
        if (previous) previous(0);
      });
    },

    audioDecodeStreamResult(handle) {
      const s = streams.get(handle);
      if (!s) return new Float32Array(0);
      let words;
      if (!s.ready || s.readyKind !== s.operation) {
        words = s.operation === READ ? Float32Array.of(-2, 0, 0) : Float32Array.of(0);
      } else if (s.readyKind === REPOSITION) {
        words = Float32Array.of(s.status);
      } else {
        words = new Float32Array(s.samples.length + 3);
        words[0] = s.status;
        words[1] = s.rate;
        words[2] = s.channels;
        words.set(s.samples, 3);
      }
      const prefetch = s.ready && s.readyKind === READ && s.status === 1 && !s.closed;
      s.ready = false;
      s.operation = NONE;
      s.samples = new Float32Array(0);
      if (prefetch) {
        s.request = READ;
        schedule(s);
      }
      return words;
    },

    audioDecodeStreamDuration(handle) {
      const s = streams.get(handle);
      return s ? s.duration : -1;
    },

    audioDecodeStreamSeekOffset(handle, frame) {
      const s = streams.get(handle);
      const layout = s && s.layout;
      const target = BigInt(frame);
      if (!layout || !layout.blockAlign || target < 0n || target > layout.frames ||
          target > (INT64_MAX - layout.dataPosition) / layout.blockAlign) {
        return -1n;
      }
      return layout.dataPosition + target * layout.blockAlign;
    },

    audioDecodeStreamClose(handle) {
      const s = streams.get(handle);
      if (!s) return;
      streams.delete(handle);
      s.closed = true;
      s.request = NONE;
      if (!s.running) streaming.send({ t: "close", key: s.key });
    },
  };
}
