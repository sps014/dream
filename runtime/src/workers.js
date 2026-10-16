import { isNode } from "./platform.js";

/**
 * Source of the worker bootstrap module. It imports this same `dream.js` (so the worker reuses
 * all the env/`Dream` import wiring, including nested workers) and, on `init`, instantiates the
 * same `.wasm` bytes importing the parent's shared `WebAssembly.Memory`. Thereafter each `msg` or
 * `dispatch` runs a `fun(string):string` body via `__workerInvoke` and posts the reply back.
 *
 * Browser workers use `self.onmessage` / `self.postMessage`; Node `worker_threads` workers use
 * `parentPort` instead — pass `node: true` for that dialect.
 */
function unpackWire(data) {
  if (data == null || data === "") {
    return "";
  }
  if (typeof data === "string") {
    return data;
  }
  const u =
    data instanceof Uint16Array
      ? data
      : new Uint16Array(data.buffer, data.byteOffset, Math.floor(data.byteLength / 2));
  if (u.length <= 8192) {
    return String.fromCharCode.apply(null, u);
  }
  let s = "";
  for (let i = 0; i < u.length; i += 8192) {
    s += String.fromCharCode.apply(null, u.subarray(i, i + 8192));
  }
  return s;
}

function packWire(s) {
  const n = s == null ? 0 : s.length;
  const u = new Uint16Array(n);
  for (let i = 0; i < n; i++) {
    u[i] = s.charCodeAt(i);
  }
  return u;
}

const WORKER_BOOT_UNPACK = `function unpackWire(data) {
  if (data == null || data === "") return "";
  if (typeof data === "string") return data;
  const u = data instanceof Uint16Array
    ? data
    : new Uint16Array(data.buffer, data.byteOffset, Math.floor(data.byteLength / 2));
  if (u.length <= 8192) return String.fromCharCode.apply(null, u);
  let s = "";
  for (let i = 0; i < u.length; i += 8192) s += String.fromCharCode.apply(null, u.subarray(i, i + 8192));
  return s;
}
function packWire(s) {
  const n = s == null ? 0 : s.length;
  const u = new Uint16Array(n);
  for (let i = 0; i < n; i++) u[i] = s.charCodeAt(i);
  return u;
}
`;

function workerBootSource(dreamUrl, { node = false } = {}) {
  if (node) {
    return `import { parentPort } from 'node:worker_threads';
import * as Dream from ${JSON.stringify(dreamUrl)};
${WORKER_BOOT_UNPACK}
let inst = null;
let chain = Promise.resolve();
parentPort.on('message', (m) => {
  chain = chain.then(async () => {
    if (m.t === 'init') {
      inst = await Dream.load(m.bytes, { abi: m.abi, memory: m.memory, stackGate: m.stackGate });
      parentPort.postMessage({ t: 'ready', stack: inst.workerStack || 0 });
    } else if (m.t === 'reset') {
      inst.__workerReset();
    } else if (m.t === 'dispatch') {
      const reply = await inst.__workerInvoke(m.fnIdx, m.env, unpackWire(m.data));
      parentPort.postMessage({ t: 'reply', data: packWire(reply), idle: inst._hostPending === 0 });
    } else if (m.t === 'term') {
      await inst.__workerClose();
      parentPort.postMessage({ t: 'closed' });
      parentPort.close();
    }
  });
});
`;
  }
  return `import * as Dream from ${JSON.stringify(dreamUrl)};
${WORKER_BOOT_UNPACK}
let inst = null;
let chain = Promise.resolve();
self.onmessage = (e) => {
  const m = e.data;
  chain = chain.then(async () => {
    if (m.t === 'init') {
      inst = await Dream.load(m.bytes, { abi: m.abi, memory: m.memory, stackGate: m.stackGate });
      self.postMessage({ t: 'ready', stack: inst.workerStack || 0 });
    } else if (m.t === 'reset') {
      inst.__workerReset();
    } else if (m.t === 'dispatch') {
      const reply = await inst.__workerInvoke(m.fnIdx, m.env, unpackWire(m.data));
      self.postMessage({ t: 'reply', data: packWire(reply), idle: inst._hostPending === 0 });
    } else if (m.t === 'term') {
      await inst.__workerClose();
      self.postMessage({ t: 'closed' });
      self.close();
    }
  });
};
`;
}

/**
 * Builds the worker session/dispatch hosts behind `system.task`'s `Task` and `TaskPool`.
 * Each slot runs a separate instance with shared memory; one-shot sessions retire before
 * their reply resumes Dream. Dispatch returns a Promise bridged into Dream's scheduler.
 */
function makeWorkerModule(wasmBytes, abi, getSharedMemory, stackGate, getInstance) {
  const reg = new Map();
  const stopping = new Set();
  let nextId = 1;
  /** Lazily resolved Node `worker_threads.Worker` constructor (null until first Node spawn). */
  let NodeWorkerCtor = null;
  /**
   * Idle workers kept for the next session. Instantiating the module per Task dominated short
   * fan-outs, and each instance's stack stays carved out of shared memory until its worker has
   * stopped, so churning workers could exhaust the heap. A reused worker restarts its stack,
   * thread-locals, and private heap in place, so each session still starts as a fresh thread.
   */
  const idle = [];
  const IDLE_MAX = 64;

  // A slot is one worker; `slot.state` is the session it currently serves.
  const postJob = (state, job) => {
    const slot = state.slot;
    state.inflight++;
    if (slot.ready) slot.worker.postMessage(job);
    else slot.queued.push(job);
  };

  const onWorkerMessage = (slot, m) => {
    if (m.t === "ready") {
      slot.ready = true;
      slot.stack = Number(m.stack ?? 0);
      if (slot.terminateWhenReady) {
        slot.queued = [];
        slot.terminateWhenReady();
        return;
      }
      for (const q of slot.queued) slot.worker.postMessage(q);
      slot.queued = [];
    } else if (m.t === "closed") {
      slot.closed?.();
    } else if (m.t === "reply") {
      const state = slot.state;
      slot.idle = m.idle;
      if (!state) return;
      state.inflight--;
      const text = unpackWire(m.data);
      if (state.pending.length > 0) state.pending.shift()(text);
      if (state.oneShot) terminateWorker(state.id);
    }
  };

  const attachHandlers = (worker, slot) => {
    // Browser: `onmessage` event with `e.data`. Node worker_threads: `message` event with data directly.
    if (typeof worker.on === "function" && isNode) {
      worker.on("message", (m) => onWorkerMessage(slot, m));
    } else {
      worker.onmessage = (e) => onWorkerMessage(slot, e.data);
    }
  };

  const publish = (ptr) => {
    const n = Number(ptr ?? 0);
    if (!n) return;
    if (typeof getInstance !== "function") {
      throw new Error("dream_publish: Wasm instance is not ready");
    }
    const inst = getInstance();
    const pub = inst && inst.exports && inst.exports.dream_publish;
    if (typeof pub !== "function") {
      throw new Error("dream_publish export is missing");
    }
    pub(n);
  };

  const startWorker = (state) => {
    const slot = {
      worker: null,
      state,
      ready: false,
      queued: [],
      blobUrl: null,
      stack: 0,
      idle: true,
    };

    const finishSpawn = (worker) => {
      slot.worker = worker;
      attachHandlers(worker, slot);
      worker.postMessage({
        t: "init",
        bytes: wasmBytes,
        abi,
        memory: getSharedMemory(),
        stackGate,
      });
    };

    if (isNode) {
      // Spawn synchronously once the ctor is cached; first call kicks off an async import and
      // queues the init until it resolves (postJob already buffers until `ready`).
      const startNode = (NodeWorker) => {
        const worker = new NodeWorker(workerBootSource(import.meta.url, { node: true }), {
          eval: true,
        });
        finishSpawn(worker);
      };
      if (NodeWorkerCtor) {
        startNode(NodeWorkerCtor);
      } else {
        // Synchronous-looking spawn from Dream's POV: register the id immediately; jobs queue
        // until the worker posts `ready`. The dynamic import of worker_threads is one-shot.
        import("node:worker_threads").then(({ Worker }) => {
          NodeWorkerCtor = Worker;
          startNode(Worker);
        });
      }
    } else if (typeof Worker !== "undefined") {
      const url = URL.createObjectURL(
        new Blob([workerBootSource(import.meta.url)], { type: "text/javascript" }),
      );
      slot.blobUrl = url;
      finishSpawn(new Worker(url, { type: "module" }));
    } else {
      throw new Error(
        "Task needs a thread runtime (browser Worker or Node worker_threads); neither is available in this environment",
      );
    }
    return slot;
  };

  const spawnWorker = (oneShot) => {
    const state = {
      slot: null,
      oneShot: !!oneShot,
      id: 0,
      pending: [],
      inflight: 0,
    };
    const reused = idle.pop();
    if (reused) {
      reused.state = state;
      state.slot = reused;
      if (typeof reused.worker.ref === "function") reused.worker.ref();
      reused.worker.postMessage({ t: "reset" });
    } else {
      state.slot = startWorker(state);
    }
    const id = nextId++;
    state.id = id;
    reg.set(id, state);
    return id;
  };

  const terminateWorker = (id) => {
    const s = reg.get(id);
    if (!s) return;
    reg.delete(id);
    for (const p of s.pending) p("");
    const slot = s.slot;
    // A worker mid-job, or whose last job left host calls running, cannot be handed on: they
    // would settle into the next session's thread.
    if (slot.worker && slot.ready && slot.idle && s.inflight === 0 && idle.length < IDLE_MAX) {
      slot.state = null;
      // An idle Node worker must not keep the process alive.
      if (typeof slot.worker.unref === "function") slot.worker.unref();
      idle.push(slot);
      return;
    }
    const revokeBlob = () => {
      if (!slot.blobUrl) return;
      try {
        URL.revokeObjectURL(slot.blobUrl);
      } catch (_) {
        /* ignore */
      }
    };
    const releaseStack = () => {
      if (slot.stack && typeof getInstance === "function") {
        getInstance().exports.dream_wasm_raw_free(slot.stack);
      }
    };
    if (slot.worker || !slot.ready) {
      const stopped = new Promise((resolve) => {
        const terminate = () => {
          const result = slot.worker.terminate();
          if (result && typeof result.then === "function") {
            result.then(() => { releaseStack(); resolve(); }, resolve);
          } else { releaseStack(); resolve(); }
        };
        // A booting worker may hold the shared bootstrap gate, and killing it there would leave
        // every later worker waiting on that gate forever. It retires once it reports ready.
        // Finished bodies retire gracefully; a busy body still needs immediate termination.
        if (!slot.ready) {
          // The browser may not have fetched the boot script from its blob URL yet.
          slot.terminateWhenReady = () => { revokeBlob(); terminate(); };
        } else if (s.inflight === 0) {
          slot.closed = terminate;
          slot.worker.postMessage({ t: "term" });
        } else { terminate(); }
      });
      stopping.add(stopped);
      stopped.then(() => stopping.delete(stopped));
    }
    if (slot.ready) revokeBlob();
  };

  return {
    workerSpawn: spawnWorker,
    workerDispatch: (id, fnIndex, env, msg) =>
      new Promise((resolve) => {
        const s = reg.get(id);
        if (!s) return resolve("");
        publish(env);
        s.pending.push(resolve);
        postJob(s, { t: "dispatch", fnIdx: fnIndex, env, data: packWire(msg) });
      }),
    // JS termination never blocks the caller, so there is nothing to overlap.
    __drainWorkers: async () => {
      while (stopping.size) await Promise.all([...stopping]);
    },
    workerShutdown: () => {},
    workerTerminate: terminateWorker,
  };
}
