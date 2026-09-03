import { TAGS, elementSize, stripSuffix, parseFunType, jsToWasm, wasmToJs } from "./core.js";

export class DreamInstance {
  constructor(instance) {
    this.instance = instance;
    this.exports = instance.exports;
    this.memory = instance.exports.memory;
    // JS-object handle registry backing the Dream `js` type. A `js` value crosses the boundary
    // as a small i32 id; the host keeps the real JS value here with a Dream-owned refcount.
    // Id 0 is reserved for null. `registerHandle` / `retainValue` / `releaseValue` keep the count
    // in sync with MIR `Retain`/`Release` so the entry is dropped when the last owner releases.
    this._jsHandles = new Map(); // id -> { value, count }
    this._jsIds = new Map(); // JS value -> id (identity for objects, value for primitives)
    this._jsNextId = 1;
    this._jsFreeIds = [];
    // Cache of JS callables wrapping Dream funcrefs, keyed by `${index}|${typeStr}`. Funcrefs are
    // captureless table indices, so index identity == function identity: returning the *same* JS
    // callable for the same funcref lets `addEventListener`/`removeEventListener` pair correctly.
    this._callbackWrappers = new Map();
    this.workerStack = 0;
  }

  /**
   * Registers a JS value, returning its `js` handle id (0 for null/undefined).
   * Each call hands Dream a +1: a fresh entry starts at count 1; an existing identity bumps count.
   */
  registerHandle(value) {
    if (value === null || value === undefined) return 0;
    const existing = this._jsIds.get(value);
    if (existing !== undefined) {
      const entry = this._jsHandles.get(existing);
      entry.count += 1;
      return existing;
    }
    const id = this._jsFreeIds.length ? this._jsFreeIds.pop() : this._jsNextId++;
    this._jsHandles.set(id, { value, count: 1 });
    this._jsIds.set(value, id);
    return id;
  }

  /** Resolves a `js` handle id back to its JS value (null for id 0 / unknown). */
  derefHandle(id) {
    if (!id) return null;
    const entry = this._jsHandles.get(id);
    return entry ? entry.value : null;
  }

  /** Bumps the host refcount for an already-registered value (Dream `Retain` of a borrowed `js`). */
  retainValue(value) {
    if (value === null || value === undefined) return;
    const id = this._jsIds.get(value);
    if (id === undefined) return;
    this._jsHandles.get(id).count += 1;
  }

  /** Drops one Dream ownership of `value`; deletes the registry entry when the count hits 0. */
  releaseValue(value) {
    if (value === null || value === undefined) return;
    const id = this._jsIds.get(value);
    if (id === undefined) return;
    const entry = this._jsHandles.get(id);
    entry.count -= 1;
    if (entry.count > 0) return;
    this._jsHandles.delete(id);
    this._jsIds.delete(value);
    this._jsFreeIds.push(id);
  }

  /** A fresh DataView over current memory (memory may grow, so do not cache the buffer). */
  get view() {
    return new DataView(this.memory.buffer);
  }

  /** A fresh Uint8Array over current memory. */
  get bytes() {
    return new Uint8Array(this.memory.buffer);
  }

  // --- raw scalar reads -----------------------------------------------------
  i32(ptr) {
    return this.view.getInt32(ptr, true);
  }
  f32(ptr) {
    return this.view.getFloat32(ptr, true);
  }
  f64(ptr) {
    return this.view.getFloat64(ptr, true);
  }

  /**
   * Payload address of a Dream string. C owned strings store pad 0 (units at `ptr+8`);
   * C slices store pad 1 and the units pointer at `ptr+8+4` on wasm32; WAT stores the
   * absolute units address in pad.
   */
  stringUnitsStart(ptr) {
    const pad = this.view.getInt32(ptr + 4, true);
    if (pad === 0) return ptr + 8;
    if (pad === 1) return this.view.getInt32(ptr + 12, true);
    return pad;
  }

  /**
   * Reads a Dream string at `ptr` (a data pointer). Layout:
   * `[unit_len: i32][pad: i32][utf16le...]`. Uses code-unit copy so `Bytes.toWire`
   * payloads that contain U+0000 survive the JS boundary (TextDecoder is not required).
   */
  readString(ptr) {
    if (!ptr) return "";
    const n = this.view.getInt32(ptr, true);
    if (n <= 0) return "";
    const start = this.stringUnitsStart(ptr);
    const u16 = new Uint16Array(this.memory.buffer.slice(start, start + n * 2));
    if (u16.length <= 8192) {
      return String.fromCharCode.apply(null, u16);
    }
    let s = "";
    for (let i = 0; i < u16.length; i += 8192) {
      s += String.fromCharCode.apply(null, u16.subarray(i, i + 8192));
    }
    return s;
  }

  /**
   * Allocates a Dream string block for `str` and returns its data pointer, so JS-implemented
   * extern functions can return strings back into Dream. Requires the module to export `malloc`.
   * Layout: `[unit_len: i32][pad: i32][utf16le...]` (no NUL terminator).
   */
  guestMalloc(size, tag) {
    if (typeof this.exports.dream_malloc === "function") {
      return this.exports.dream_malloc(size, tag);
    }
    if (typeof this.exports.malloc !== "function") {
      throw new Error("module does not export `malloc`; cannot allocate");
    }
    return this.exports.malloc(size, tag);
  }

  writeString(str) {
    const units = str.length;
    const ptr = this.guestMalloc(8 + units * 2, TAGS.STRING);
    this.view.setInt32(ptr, units, true);
    this.view.setInt32(ptr + 4, ptr + 8, true);
    for (let i = 0; i < units; i++) {
      this.view.setUint16(ptr + 8 + i * 2, str.charCodeAt(i), true);
    }
    return ptr;
  }

  /** Reads a single element of `elemType` at byte address `addr`. */
  _readElement(addr, elemType) {
    const t = stripSuffix(elemType);
    switch (t) {
      case "int":
        return this.i32(addr);
      case "char":
      case "byte":
        return this.bytes[addr]; // 1-byte element
      case "bool":
        return this.bytes[addr] !== 0;
      case "uint":
        return this.view.getUint32(addr, true);
      case "long":
        return this.view.getBigInt64(addr, true);
      case "ulong":
        return this.view.getBigUint64(addr, true);
      case "float":
        return this.f32(addr);
      case "double":
        return this.f64(addr);
      case "string":
        return this.readString(this.i32(addr));
      default:
        if (t.endsWith("[]")) return this.readArray(this.i32(addr), t.slice(0, -2));
        return this.i32(addr); // struct/object/list: opaque pointer
    }
  }

  /** Writes a single element of `elemType` at byte address `addr`. */
  _writeElement(addr, elemType, value) {
    const t = stripSuffix(elemType);
    switch (t) {
      case "int":
        this.view.setInt32(addr, value | 0, true);
        break;
      case "char":
      case "byte":
        this.bytes[addr] = value & 0xff; // 1-byte element
        break;
      case "bool":
        this.bytes[addr] = value ? 1 : 0;
        break;
      case "uint":
        this.view.setUint32(addr, value >>> 0, true);
        break;
      case "long":
        this.view.setBigInt64(addr, BigInt(value == null ? 0 : value), true);
        break;
      case "ulong":
        this.view.setBigUint64(addr, BigInt(value == null ? 0 : value), true);
        break;
      case "float":
        this.view.setFloat32(addr, value, true);
        break;
      case "double":
        this.view.setFloat64(addr, value, true);
        break;
      case "string":
        this.view.setInt32(addr, this.writeString(value == null ? "" : String(value)), true);
        break;
      default:
        this.view.setInt32(addr, value | 0, true); // struct/object pointer
    }
  }

  /**
   * Allocates a Dream array from a JS array (or typed array) of `elemType`, returning its data
   * pointer, so JS-implemented externs can return arrays (e.g. `char[]` file bytes) back into
   * Dream. Layout: [count:i32] followed by `count` elements. Requires the module to export `malloc`.
   */
  writeArray(arr, elemType = "int") {
    const elem = stripSuffix(elemType);
    const size = elementSize(elem);
    const count = arr.length;
    const ptr = this.guestMalloc(4 + count * size, TAGS.ARRAY);
    this.view.setInt32(ptr, count, true);
    if (elem === "char" || elem === "byte") {
      // Bulk copy for the common byte-array case.
      this.bytes.set(Uint8Array.from(arr), ptr + 4);
    } else {
      for (let i = 0; i < count; i++) {
        this._writeElement(ptr + 4 + i * size, elem, arr[i]);
      }
    }
    return ptr;
  }

  /**
   * Reads a Dream array at data pointer `ptr` into a JS array. Layout: [count:i32] followed by
   * `count` elements of `elemType`.
   */
  readArray(ptr, elemType = "int") {
    if (!ptr) return [];
    const count = this.i32(ptr);
    const size = elementSize(elemType);
    const out = new Array(count);
    for (let i = 0; i < count; i++) {
      out[i] = this._readElement(ptr + 4 + i * size, elemType);
    }
    return out;
  }

  /**
   * Reads a `List<T>` at data pointer `ptr` into a JS array. A List is a struct `{ items: T[];
   * count: int }`, so `items` is at offset 0 and the logical length at offset 4.
   */
  readList(ptr, elemType = "int") {
    if (!ptr) return [];
    const itemsPtr = this.i32(ptr);
    const count = this.i32(ptr + 4);
    const size = elementSize(elemType);
    const out = new Array(count);
    for (let i = 0; i < count; i++) {
      out[i] = this._readElement(itemsPtr + 4 + i * size, elemType);
    }
    return out;
  }

  /**
   * Reads a struct at data pointer `ptr` using a schema describing its fields in declaration
   * order. Schema entries are `{ name, type }`; offsets are derived from element sizes.
   */
  readStruct(ptr, schema) {
    const out = {};
    let offset = 0;
    for (const field of schema) {
      out[field.name] = this._readElement(ptr + offset, field.type);
      offset += elementSize(field.type);
    }
    return out;
  }

  /**
   * Wraps a Dream function value (an `i32` index into the exported `__indirect_function_table`)
   * as a JS callable, so a Dream function passed to a `fun(...)`-typed extern parameter can be
   * invoked by the host. `typeStr` is the Dream function type (e.g. `fun(int):void`) used to
   * marshal arguments in and the result out.
   */
  callback(index, typeStr = "fun():void") {
    if (index < 0) return null;
    const table = this.exports.__indirect_function_table;
    if (!table) throw new Error("module does not export its function table; cannot build a callback");
    // C-backend modules number `fun` values by their internal `dream_ft[]` dispatch table, whose
    // slots clang assigns independently of the wasm table. The exported `dream_ft_get` maps an ft
    // index to the function-pointer value, which on wasm32 *is* the table index. WAT-backend
    // modules (and workers, which dispatch through `dream_ft` themselves) need no translation.
    const mapFt = this.exports.dream_ft_get;
    const tableIndex = typeof mapFt === "function" ? Number(mapFt(index)) : index;
    const cacheKey = `${index}|${typeStr}`;
    const cached = this._callbackWrappers.get(cacheKey);
    if (cached) return cached;
    const fn = table.get(tableIndex);
    if (typeof fn !== "function") {
      throw new Error(`no Dream function at table index ${tableIndex}`);
    }
    const { params, result } = parseFunType(typeStr);
    const wrapper = (...jsArgs) => {
      const raw = params.map((p, i) => jsToWasm(this, p, jsArgs[i]));
      const out = fn(...raw);
      return wasmToJs(this, result, out);
    };
    // Expose the raw table index so callers that need the portable funcref value itself (e.g.
    // `WebWorker` shipping a body to another instance of the same module) can recover it.
    wrapper.__dreamFuncIndex = index;
    this._callbackWrappers.set(cacheKey, wrapper);
    return wrapper;
  }

  /**
   * Runs one `fun(string): string` worker body: writes `msg` into this instance's memory, calls the
   * exported `__dream_worker_invoke_raw` trampoline (publishes `env` to the closure-env global, then
   * a single `call_indirect` on the body funcref `fnIndex`), and resolves with the reply string.
   * Used by the Web Worker / Node worker_threads bootstrap. Returns a Promise because an *async*
   * body's `call_indirect` only returns its `Future` constructor's frame pointer, not the real
   * reply — see `__awaitWorkerResult`.
   *
   * Workers (browser and Node) import the parent's shared `WebAssembly.Memory`, so an `@shared
   * class` / unmanaged `env` pointer is meaningful across threads. Browser pages still need
   * COOP/COEP headers for `SharedArrayBuffer`; Node's `worker_threads` exposes it by default.
   */
  __workerInvoke(fnIndex, env, msg) {
    const ptr = this.writeString(msg == null ? "" : String(msg));
    const r = this.exports.__dream_worker_invoke_raw(
      Number(fnIndex),
      Number(env ?? 0),
      Number(ptr),
    );
    return this.__awaitWorkerResult(r);
  }

  /** Heap type tag with `TAG_SHARED` stripped (`HEADER_TAG_OFFSET` is 8 bytes before the payload). */
  __valueTag(ptr) {
    return this.i32(ptr - 8) & 0x3fffffff;
  }

  __isFutureFrame(ptr) {
    return !!ptr && this.__valueTag(ptr) === TAGS.FUTURE;
  }

  __guestFree(ptr) {
    if (!ptr || typeof this.exports.free !== "function") return;
    this.exports.free(ptr);
  }

  /**
   * Interprets a `__dream_worker_invoke_raw` return value: a `TAG_FUTURE` frame means the body was
   * `async fun` and `r` is still running. A string (or other tagged heap value) is the reply.
   *
   * Unlike native (where every host `async` op resolves synchronously before `call_indirect`
   * returns, so one `__dream_run_loop` pass always finishes the task), a real `extern async` host
   * call here only settles later via a Promise `.then()` callback (see `wrapAsyncImport`), which
   * itself re-pumps `__dream_run_loop`. So instead of draining once, poll the future's `F_STATUS`
   * slot until some pump marks it done, then unwrap `F_RESULT`.
   */
  __awaitWorkerResult(r) {
    const F_RESULT = 8;
    if (!r) return Promise.resolve("");
    if (!this.__isFutureFrame(r)) {
      const text = this.readString(r);
      this.__guestFree(r);
      return Promise.resolve(text);
    }
    return this.__awaitFuture(r).then(() => {
      const out = this.i32(r + F_RESULT);
      const text = this.readString(out);
      this.__guestFree(r);
      return text;
    });
  }

  /**
   * Waits until a `TAG_FUTURE` frame is settled. `wrapAsyncImport` re-pumps `__dream_run_loop`
   * when the host Promise completes; this only observes `F_STATUS`.
   */
  __awaitFuture(r) {
    const F_STATUS = 4; // mirrors src/mir/async_emit.rs
    if (!r || this.i32(r + F_STATUS) !== 0) return Promise.resolve();
    return new Promise((resolve) => {
      // `setTimeout`, not `queueMicrotask`: a real pending host op (e.g. `fetch`) settles via a
      // macrotask-queued I/O callback, which a tight microtask-only poll loop would starve (the
      // microtask queue must fully drain before the next macrotask runs), hanging forever.
      const poll = () => {
        if (typeof this.exports.__dream_run_loop === "function") {
          this.exports.__dream_run_loop();
        }
        if (this.i32(r + F_STATUS) !== 0) resolve();
        else setTimeout(poll, 0);
      };
      poll();
    });
  }

  /** Process-exit global `del` / RC drop. Native does this in `dream_guest_entry`; wasm32 does not. */
  __dropGlobals() {
    const drop = this.exports.__dream_drop_globals;
    if (typeof drop === "function") drop();
  }

  /** Calls the exported `main`, if present. Async `main` returns a Future pointer. */
  run() {
    if (typeof this.exports.main !== "function") {
      throw new Error("module has no exported `main`");
    }
    const r = this.exports.main();
    const after = () => {
      if (this.__isFutureFrame(r) && typeof this.exports.free === "function") {
        this.exports.free(r);
      }
      this.__dropGlobals();
    };
    if (!r) {
      after();
      return Promise.resolve();
    }
    if (!this.__isFutureFrame(r)) {
      after();
      return Promise.resolve(r);
    }
    return this.__awaitFuture(r).then(() => {
      after();
    });
  }
}
