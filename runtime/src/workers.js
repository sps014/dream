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

// Structured clone copies short strings faster than allocating and filling a Uint16Array.
const WIRE_PACK_MIN = 256;

function packWire(s) {
  const n = s == null ? 0 : s.length;
  if (n < WIRE_PACK_MIN) {
    return s ?? "";
  }
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
  if (n < ${WIRE_PACK_MIN}) return s ?? "";
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
  }).catch((err) => {
    parentPort.postMessage({ t: 'error', phase: m.t, message: String((err && err.stack) || err) });
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
  }).catch((err) => {
    self.postMessage({ t: 'error', phase: m.t, message: String((err && err.stack) || err) });
  });
};
`;
}

/**
 * Generational session handles: `(generation << 23) | slot`, generation 1..255, so a handle is
 * always a positive i32 and ids never run out. Freed slots are reused oldest-first, so a stale
 * handle misses until its slot has cycled through the whole free queue 255 times.
 */
function makeSessionTable() {
  const GEN_SHIFT = 23;
  const SLOT_MASK = (1 << GEN_SHIFT) - 1;
  const entries = [];
  const free = [];
  let freeHead = 0;
  const entryOf = (id) => {
    if (!(id > 0)) return undefined;
    const e = entries[id & SLOT_MASK];
    return e && e.state !== null && e.gen === id >>> GEN_SHIFT ? e : undefined;
  };
  return {
    insert(state) {
      let slot;
      if (freeHead < free.length) {
        slot = free[freeHead++];
        if (freeHead >= 1024 && freeHead * 2 >= free.length) {
          free.splice(0, freeHead);
          freeHead = 0;
        }
      } else {
        slot = entries.length;
        if (slot > SLOT_MASK) return 0;
        entries.push({ gen: 1, state: null });
      }
      const e = entries[slot];
      e.state = state;
      return (e.gen << GEN_SHIFT) | slot;
    },
    get(id) {
      return entryOf(id)?.state;
    },
    remove(id) {
      const e = entryOf(id);
      if (!e) return undefined;
      const state = e.state;
      e.state = null;
      e.gen = e.gen === 255 ? 1 : e.gen + 1;
      free.push(id & SLOT_MASK);
      return state;
    },
  };
}

/**
 * Builds the worker session/dispatch hosts behind `system.task`'s `Task` and `TaskPool`.
 * Each slot runs a separate instance with shared memory; one-shot sessions retire before
 * their reply resumes Dream. Dispatch returns a Promise bridged into Dream's scheduler.
 *
 * A worker that fails (bootstrap rejection, crash, exit, undeliverable message) settles every
 * call it owed exactly once and records the failure as the instance's fatal host error, so the
 * awaiting program traps instead of decoding an empty reply as a result.
 */
function makeWorkerModule(wasmBytes, abi, getSharedMemory, stackGate, getInstance) {
  const sessions = makeSessionTable();
  const stopping = new Set();
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
  /** A worker retired before `ready` that still has not booted after this long is wedged. */
  const BOOT_TIMEOUT_MS = 10000;

  const instanceOrNull = () => {
    try {
      return typeof getInstance === "function" ? getInstance() : null;
    } catch (_) {
      return null;
    }
  };

  const describeFailure = (err) => {
    if (err && typeof err === "object" && "phase" in err) return `${err.phase}: ${err.message}`;
    if (err && typeof err === "object" && "message" in err) return String(err.stack || err.message);
    return String(err);
  };

  const revokeBlob = (slot) => {
    if (!slot.blobUrl) return;
    try {
      URL.revokeObjectURL(slot.blobUrl);
    } catch (_) {
      /* ignore */
    }
    slot.blobUrl = null;
  };

  const releaseStack = (slot) => {
    const stack = slot.stack;
    slot.stack = 0;
    if (!stack) return;
    const inst = instanceOrNull();
    if (inst) inst.exports.dream_wasm_raw_free(stack);
  };

  /** Stops the worker; the stack returns to the heap only once the thread can no longer use it. */
  const killSlot = (slot) => {
    if (slot.killed) return slot.killed;
    revokeBlob(slot);
    slot.killed = new Promise((resolve) => {
      const done = () => {
        releaseStack(slot);
        resolve();
      };
      if (!slot.worker) {
        done();
        return;
      }
      let result;
      try {
        result = slot.worker.terminate();
      } catch (_) {
        done();
        return;
      }
      if (result && typeof result.then === "function") result.then(done, done);
      else done();
    });
    return slot.killed;
  };

  const finishSlot = (slot) => {
    const finish = slot.finish;
    slot.finish = null;
    if (finish) finish();
  };

  const failSlot = (slot, err) => {
    if (slot.failed) return;
    slot.failed = true;
    if (slot.bootTimer) clearTimeout(slot.bootTimer);
    slot.bootTimer = null;
    slot.queued = [];
    slot.terminateWhenReady = null;
    slot.closed = null;
    const at = idle.indexOf(slot);
    if (at >= 0) idle.splice(at, 1);
    const state = slot.state;
    slot.state = null;
    if (state && !slot.retired) {
      sessions.remove(state.id);
      const inst = instanceOrNull();
      if (inst) inst._hostError ??= new Error(`Dream task worker failed (${describeFailure(err)})`);
      const pending = state.pending;
      state.pending = [];
      for (const p of pending) p("");
    }
    killSlot(slot).then(() => finishSlot(slot));
  };

  // A slot is one worker; `slot.state` is the session it currently serves.
  const postJob = (state, job) => {
    const slot = state.slot;
    state.inflight++;
    if (slot.ready) slot.worker.postMessage(job);
    else slot.queued.push(job);
  };

  const onWorkerMessage = (slot, m) => {
    if (slot.failed) return;
    if (m.t === "ready") {
      slot.ready = true;
      slot.stack = Number(m.stack ?? 0);
      if (slot.bootTimer) clearTimeout(slot.bootTimer);
      slot.bootTimer = null;
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
    } else if (m.t === "error") {
      failSlot(slot, m);
    }
  };

  const attachHandlers = (worker, slot) => {
    // Browser: `onmessage` event with `e.data`. Node worker_threads: `message` event with data directly.
    if (typeof worker.on === "function" && isNode) {
      worker.on("message", (m) => onWorkerMessage(slot, m));
      worker.on("error", (e) => failSlot(slot, e));
      worker.on("messageerror", (e) => failSlot(slot, e));
      worker.on("exit", (code) => failSlot(slot, new Error(`worker exited with code ${code}`)));
    } else {
      worker.onmessage = (e) => onWorkerMessage(slot, e.data);
      worker.onerror = (e) => {
        if (typeof e.preventDefault === "function") e.preventDefault();
        failSlot(slot, e.error ?? new Error(e.message || "worker error"));
      };
      worker.onmessageerror = () => failSlot(slot, new Error("worker message could not be deserialized"));
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
      failed: false,
      retired: false,
      killed: null,
      finish: null,
      bootTimer: null,
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

    const spawnOrFail = (create) => {
      let worker;
      try {
        worker = create();
      } catch (err) {
        failSlot(slot, err);
        return;
      }
      finishSpawn(worker);
    };

    if (isNode) {
      // Spawn synchronously once the ctor is cached; first call kicks off an async import and
      // queues the init until it resolves (postJob already buffers until `ready`).
      const startNode = (NodeWorker) =>
        spawnOrFail(() => new NodeWorker(workerBootSource(import.meta.url, { node: true }), { eval: true }));
      if (NodeWorkerCtor) {
        startNode(NodeWorkerCtor);
      } else {
        // Synchronous-looking spawn from Dream's POV: register the id immediately; jobs queue
        // until the worker posts `ready`. The dynamic import of worker_threads is one-shot.
        import("node:worker_threads").then(
          ({ Worker }) => {
            NodeWorkerCtor = Worker;
            if (!slot.failed) startNode(Worker);
          },
          (err) => failSlot(slot, err),
        );
      }
    } else if (typeof Worker !== "undefined") {
      spawnOrFail(() => {
        const url = URL.createObjectURL(
          new Blob([workerBootSource(import.meta.url)], { type: "text/javascript" }),
        );
        slot.blobUrl = url;
        return new Worker(url, { type: "module" });
      });
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
    const id = sessions.insert(state);
    if (!id) return 0;
    state.id = id;
    const reused = idle.pop();
    if (reused) {
      reused.state = state;
      state.slot = reused;
      if (typeof reused.worker.ref === "function") reused.worker.ref();
      reused.worker.postMessage({ t: "reset" });
    } else {
      state.slot = startWorker(state);
    }
    return id;
  };

  const terminateWorker = (id) => {
    const s = sessions.remove(id);
    if (!s) return;
    const owed = s.pending;
    s.pending = [];
    for (const p of owed) p("");
    const slot = s.slot;
    if (slot.failed) return;
    // A worker mid-job, or whose last job left host calls running, cannot be handed on: they
    // would settle into the next session's thread.
    if (slot.worker && slot.ready && slot.idle && s.inflight === 0 && idle.length < IDLE_MAX) {
      slot.state = null;
      // An idle Node worker must not keep the process alive.
      if (typeof slot.worker.unref === "function") slot.worker.unref();
      idle.push(slot);
      return;
    }
    slot.state = null;
    slot.retired = true;
    const stopped = new Promise((resolve) => {
      slot.finish = resolve;
      const stop = () => killSlot(slot).then(() => finishSlot(slot));
      // A booting worker may hold the shared bootstrap gate, and killing it there would leave
      // every later worker waiting on that gate forever. It retires once it reports ready, or
      // fails once the boot timeout shows it never will.
      // Finished bodies retire gracefully; a busy body still needs immediate termination.
      if (!slot.ready) {
        slot.terminateWhenReady = stop;
        slot.bootTimer = setTimeout(
          () => failSlot(slot, new Error("worker bootstrap timed out")),
          BOOT_TIMEOUT_MS,
        );
        if (typeof slot.bootTimer?.unref === "function") slot.bootTimer.unref();
      } else if (s.inflight === 0) {
        slot.closed = stop;
        slot.worker.postMessage({ t: "term" });
      } else {
        stop();
      }
    });
    stopping.add(stopped);
    stopped.then(() => stopping.delete(stopped));
  };

  return {
    workerSpawn: spawnWorker,
    workerDispatch: (id, fnIndex, env, msg) =>
      new Promise((resolve) => {
        const s = sessions.get(id);
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
