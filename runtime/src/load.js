import { TAGS, HEAP_HEADER_SIZE } from "./core.js";
import { DreamInstance } from "./instance.js";
import {
  wrapImport,
  wrapAsyncImport,
  wrapInPlaceByteArrayFill,
  resolveGlobal,
} from "./marshal.js";
import { isNode, setNodeFs, setNodeCrypto, setNodeChildProcess, setNodeNet } from "./platform.js";
import { defaultEnv } from "./hosts/env.js";
import { defaultDreamModule } from "./hosts.js";
import { csprngBytes } from "./hosts/crypto.js";
import { makeWorkerModule } from "./workers.js";
import { readEmbeddedAbi, replaceArtifactExt } from "./urls.js";

export { TAGS, HEAP_HEADER_SIZE, DreamInstance };

const FALLBACK_INITIAL_MEMORY_PAGES = 64;
const FALLBACK_MAX_MEMORY_PAGES = 65536;

async function fetchBytes(source) {
  if (source instanceof ArrayBuffer) return new Uint8Array(source);
  if (source instanceof Uint8Array) return source;
  if (!isNode && typeof fetch === "function") {
    const res = await fetch(source);
    if (!res.ok) throw new Error(`failed to fetch ${source}: ${res.status}`);
    return new Uint8Array(await res.arrayBuffer());
  }
  const { readFile } = await import("node:fs/promises");
  return new Uint8Array(await readFile(source));
}

async function loadAbi(abi) {
  if (!abi) return null;
  if (typeof abi === "object" && abi.externs) return abi;
  const bytes = await fetchBytes(abi);
  return JSON.parse(new TextDecoder("utf-8").decode(bytes));
}

async function resolveAbi(wasmModule, source, options) {
  if (options.abi && typeof options.abi === "object" && options.abi.externs) {
    return options.abi;
  }
  const embedded = readEmbeddedAbi(wasmModule);
  if (embedded) return embedded;
  const url =
    typeof options.abi === "string" ? options.abi : replaceArtifactExt(source, ".abi.json");
  return loadAbi(url);
}

/**
 * Scans the wasm binary's import section for an `env`/`memory` import and reads its limits
 * flags (bit 1 = shared). Engines do not reliably expose `.type.shared` on
 * `WebAssembly.Module.imports()` entries, so this is the dependable signal.
 */
function memoryImportIsShared(bytes) {
  const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  let pos = 8; // magic + version
  const leb = () => {
    let result = 0;
    let shift = 0;
    for (;;) {
      const b = u8[pos++];
      result += (b & 0x7f) * Math.pow(2, shift);
      if (!(b & 0x80)) return result;
      shift += 7;
    }
  };
  const limits = () => {
    const flags = leb();
    leb(); // min
    if (flags & 0x01) leb(); // max
    return flags;
  };
  while (pos < u8.length) {
    const id = u8[pos++];
    const size = leb();
    if (id !== 2) {
      pos += size; // not the import section
      continue;
    }
    const end = pos + size;
    const count = leb();
    for (let i = 0; i < count && pos < end; i++) {
      // NB: `pos += leb()` would read `pos` before `leb()` runs — but leb() mutates pos
      // through the closure, so the skip would land one byte short. Stage lengths first.
      const moduleLen = leb();
      pos += moduleLen;
      const nameLen = leb();
      pos += nameLen;
      const kind = u8[pos++];
      if (kind === 2) {
        return (limits() & 0x02) !== 0; // shared flag
      } else if (kind === 0) {
        leb(); // func: type index
      } else if (kind === 1) {
        pos += 1; // table: element type
        limits();
      } else if (kind === 3) {
        pos += 2; // global: valtype + mutability
      } else {
        break; // unknown kind — stop scanning rather than desync
      }
    }
    return false;
  }
  return false;
}

function moduleWantsSharedMemory(wasmModule, desc) {
  if (desc && desc.shared) {
    return true;
  }
  // `Module.imports()[].type` is missing in some browsers. Shared-memory modules (WebWorker)
  // still import the worker hosts; use either signal.
  return WebAssembly.Module.imports(wasmModule).some(
    (i) =>
      (i.kind === "memory" && i.module === "env" && i.name === "memory" && i.type && i.type.shared) ||
      (i.kind === "function" &&
        i.module === "Dream" &&
        (i.name === "workerSpawn" ||
          i.name === "workerPost" ||
          i.name === "workerRecv" ||
          i.name === "workerTerminate" ||
          i.name === "workerPoolSpawn" ||
          i.name === "workerPoolDispatch")),
  );
}

function memoryIsShared(memory) {
  return (
    typeof SharedArrayBuffer !== "undefined" &&
    memory.buffer instanceof SharedArrayBuffer
  );
}

function makeLinearMemory(wasmModule, wasmBytes) {
  const memoryImport = WebAssembly.Module.imports(wasmModule).find(
    (i) => i.module === "env" && i.name === "memory" && i.kind === "memory",
  );
  const desc = memoryImport && memoryImport.type;
  return new WebAssembly.Memory({
    initial: desc && desc.minimum != null ? desc.minimum : FALLBACK_INITIAL_MEMORY_PAGES,
    maximum: desc && desc.maximum != null ? desc.maximum : FALLBACK_MAX_MEMORY_PAGES,
    shared:
      moduleWantsSharedMemory(wasmModule, desc) || memoryImportIsShared(wasmBytes),
  });
}

/**
 * Loads and instantiates a Dream module.
 *
 * @param {string|ArrayBuffer|Uint8Array} source - URL/path to `.wasm`, or raw bytes.
 * @param {object} [options]
 * @param {object} [options.imports] - JS implementations keyed by extern function name.
 * @param {string|object} [options.abi] - URL/path to (or parsed) `.abi.json` for auto-marshaling.
 * @param {function} [options.stdout] - Custom output sink for print builtins.
 * @param {function} [options.dreamHosts] - Optional host-factory composer; defaults to full module.
 * @returns {Promise<DreamInstance>}
 */
export async function load(source, options = {}) {
  const wasmBytes = await fetchBytes(source);
  const wasmModule = await WebAssembly.compile(wasmBytes);
  const abi = await resolveAbi(wasmModule, source, options);

  if (isNode && !options.__skipNodePreload) {
    try {
      const fs = await import("node:fs");
      setNodeFs(fs);
    } catch (_) { /* leave unavailable */ }
    try {
      const crypto = await import("node:crypto");
      setNodeCrypto(crypto);
    } catch (_) { /* leave unavailable */ }
    try {
      const childProcess = await import("node:child_process");
      setNodeChildProcess(childProcess);
    } catch (_) { /* leave unavailable */ }
    try {
      const net = await import("node:net");
      setNodeNet(net);
    } catch (_) { /* leave unavailable */ }
  }

  let instance = null;
  const getInstance = () => {
    if (!instance) throw new Error("instance not ready");
    return instance;
  };

  const importObject = { env: defaultEnv(getInstance, options) };
  const sharedMemory = options.memory ?? makeLinearMemory(wasmModule, wasmBytes);
  importObject.env.memory = sharedMemory;
  const stackGate =
    options.stackGate ??
    (memoryIsShared(sharedMemory) ? new Int32Array(new SharedArrayBuffer(4)) : null);

  const userImports = options.imports || {};
  const sigByName = new Map();
  if (abi) for (const e of abi.externs) sigByName.set(e.name, e);

  const composeHosts = options.dreamHosts || defaultDreamModule;
  // Selective runtimes omit chunk files whose hosts are unused; `typeof` guards keep
  // those references safe when the defining file was not included.
  const builtinDream = {
    ...composeHosts(getInstance),
    ...(typeof makeWorkerModule === "function"
      ? makeWorkerModule(wasmBytes, abi, () => sharedMemory, stackGate, getInstance)
      : {}),
  };
  if (typeof builtinDream.__attachGpuAbi === "function") {
    const hint =
      typeof source === "string"
        ? source
        : typeof options.abi === "string"
          ? options.abi
          : null;
    builtinDream.__attachGpuAbi(abi, hint);
  }

  const wrapFor = (fn, sig) => {
    if (
      sig &&
      sig.field === "cryptoSecureRandomFill" &&
      typeof csprngBytes === "function"
    ) {
      return wrapInPlaceByteArrayFill(getInstance, (count) => csprngBytes(count));
    }
    return sig && sig.async ? wrapAsyncImport(getInstance, fn, sig) : wrapImport(getInstance, fn, sig);
  };

  for (const name of Object.keys(userImports)) {
    const sig = sigByName.get(name);
    const module = sig ? sig.module : "env";
    const field = sig ? sig.field : name;
    (importObject[module] ||= {})[field] = wrapFor(userImports[name], sig);
  }

  if (abi) {
    for (const e of abi.externs) {
      const bucket = (importObject[e.module] ||= {});
      if (bucket[e.field]) continue;
      // `cryptoSecureRandomFill` has no direct host function: wrapFor routes it through
      // wrapInPlaceByteArrayFill + csprngBytes, so it is "implemented" despite resolving
      // to null here.
      const isCsprngFill =
        e.module === "Dream" && e.field === "cryptoSecureRandomFill" &&
        typeof csprngBytes === "function";
      const resolved = (e.module === "Dream" && builtinDream[e.field])
        ? builtinDream[e.field]
        : resolveGlobal(e.module, e.field);
      bucket[e.field] = (resolved || isCsprngFill)
        ? wrapFor(resolved, e)
        : () => {
            throw new Error(`no JS implementation for extern '${e.name}' (${e.module}.${e.field})`);
          };
    }
  }

  // Compiler-emitted imports (e.g. `jsRetain` / `jsRelease`) appear in the WASM module but are not
  // listed in `.abi.json` — bind any still-missing Dream functions from the host factory.
  // WASM passes a handle id; marshal `js` so the host sees the registered value.
  const jsRcSig = { params: ["js"], result: "void" };
  for (const imp of WebAssembly.Module.imports(wasmModule)) {
    if (imp.kind !== "function" || imp.module !== "Dream") continue;
    const bucket = (importObject.Dream ||= {});
    if (bucket[imp.name]) continue;
    const resolved = builtinDream[imp.name];
    const rcSig = (imp.name === "jsRetain" || imp.name === "jsRelease") ? jsRcSig : null;
    bucket[imp.name] = resolved
      ? wrapFor(resolved, rcSig)
      : () => {
          throw new Error(`no JS implementation for Dream.${imp.name}`);
        };
  }

  const wasmInstance = await withBootstrapLock(stackGate, async () => {
    const inst = await WebAssembly.instantiate(wasmModule, importObject);
    if (stackGate || options.memory) {
      attachGuestStack(inst);
    } else if (typeof inst.exports.__runtime_init === "function") {
      inst.exports.__runtime_init();
    }
    return inst;
  });
  instance = new DreamInstance(wasmInstance);
  return instance;
}

const WORKER_STACK_BYTES = 65536;

async function withBootstrapLock(gate, fn) {
  if (!gate) {
    return fn();
  }
  for (;;) {
    if (Atomics.compareExchange(gate, 0, 0, 1) === 0) {
      break;
    }
    if (typeof Atomics.waitAsync === "function") {
      await Atomics.waitAsync(gate, 0, 1, 50).value;
    } else {
      await new Promise((r) => setTimeout(r, 0));
    }
  }
  try {
    return await fn();
  } finally {
    Atomics.store(gate, 0, 0);
    Atomics.notify(gate, 0);
  }
}

function guestMalloc(exports, size, tag) {
  if (typeof exports.dream_malloc === "function") {
    return exports.dream_malloc(size, tag);
  }
  if (typeof exports.malloc === "function") {
    return exports.malloc(size, tag);
  }
  return 0;
}

function attachGuestStack(wasmInstance) {
  const sp = wasmInstance.exports.__stack_pointer;
  if (!sp) {
    if (typeof wasmInstance.exports.__runtime_init === "function") {
      wasmInstance.exports.__runtime_init();
    }
    return;
  }
  if (typeof wasmInstance.exports.__runtime_init === "function") {
    wasmInstance.exports.__runtime_init();
  }
  const ptr = guestMalloc(wasmInstance.exports, WORKER_STACK_BYTES, 0);
  if (!ptr) {
    throw new Error("failed to allocate a guest stack");
  }
  sp.value = ptr + WORKER_STACK_BYTES;
  const tls = wasmInstance.exports.__tls_base;
  if (tls) {
    tls.value = ptr;
  }
}

/**
 * load a module and immediately invoke its `main`.
 *
 * @returns {Promise<DreamInstance>} the loaded instance (after `main` has run).
 */
export async function run(source, options = {}) {
  const mod = await load(source, options);
  await mod.run();
  return mod;
}

export default { load, run, DreamInstance, TAGS, HEAP_HEADER_SIZE };
