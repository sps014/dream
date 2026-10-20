import { isFunType, stripSuffix } from "./core.js";
import { HAS_JS_HANDLES, HAS_CALLBACKS, HAS_ARRAYS } from "./features.js";
import { GUEST_FUNCTION_EXPORT } from "./guest_contract.js";

/** Marshals raw WASM argument values into JS values per the parameter type names. */
export function marshalArgs(inst, params, rawArgs) {
  if (!params) return rawArgs;
  return rawArgs.map((arg, i) => {
    const rawType = params[i] || "int";
    if (HAS_CALLBACKS && isFunType(rawType)) {
      return inst.callback(inst.exports[GUEST_FUNCTION_EXPORT](arg), rawType);
    }
    const t = stripSuffix(rawType);
    if (t === "string") return inst.readString(arg);
    if (HAS_JS_HANDLES && t === "js") return inst.derefHandle(arg); // i32 handle id -> live JS value
    if (HAS_ARRAYS && t.endsWith("[]")) return inst.readArray(arg, t.slice(0, -2));
    if (t === "bool") return arg !== 0;
    return arg; // numeric primitive or opaque pointer
  });
}

/** Marshals a JS return value back into the raw WASM value for the declared result type. */
export function marshalResult(inst, result, ret) {
  if (result === "string") return inst.writeString(ret == null ? "" : String(ret));
  if (result === "bool") return ret ? 1 : 0;
  if (HAS_JS_HANDLES && result === "js") return inst.registerHandle(ret); // live JS value -> i32 handle id
  if (HAS_ARRAYS && typeof result === "string" && result.endsWith("[]")) {
    return inst.writeArray(ret == null ? [] : ret, result.slice(0, -2)); // e.g. byte[] process output
  }
  if (result === "void" || result == null) return ret == null ? 0 : ret;
  return ret;
}

/** True when marshaling `t` needs the live `DreamInstance` (strings, `js`, arrays, callbacks). */
export function typeNeedsInstance(t) {
  if (!t || t === "void") return false;
  if (HAS_CALLBACKS && isFunType(t)) return true;
  const base = stripSuffix(t);
  return base === "string" || (HAS_JS_HANDLES && base === "js") || (HAS_ARRAYS && base.endsWith("[]"));
}

/** True when any param/result of an extern needs heap marshaling via `getInstance()`. */
export function signatureNeedsInstance(params, result) {
  if (typeNeedsInstance(result)) return true;
  if (params) {
    for (const p of params) {
      if (typeNeedsInstance(p)) return true;
    }
  }
  return false;
}

/** Wraps a user-provided import implementation so its args/return are marshaled per the ABI. */
export function wrapImport(getInstance, fn, signature) {
  const params = signature ? signature.params : null;
  const result = signature ? signature.result : null;
  // `(start $__runtime_init)` runs during `WebAssembly.instantiate`, before `DreamInstance` exists.
  // Pure numeric externs (e.g. `timeNow`) must not call `getInstance()` or module-level
  // constructors that touch the host die with "instance not ready".
  const needsInst = signatureNeedsInstance(params, result);

  return (...rawArgs) => {
    if (!needsInst) {
      const ret = fn(...rawArgs);
      return ret == null ? 0 : ret;
    }
    const inst = getInstance();
    const args = marshalArgs(inst, params, rawArgs);
    const ret = fn(...args);
    return marshalResult(inst, result, ret);
  };
}

/**
 * Resolves an extern import against the JS global scope so common APIs need no boilerplate.
 * The `env` module maps to a bare global (e.g. `alert`); any other module maps to a property of
 * that global object (e.g. `console.log`, `Math.max`). Returns the function bound to its owner,
 * or `undefined` if there is no matching global function.
 */
export function resolveGlobal(module, field) {
  if (module === "env") {
    const g = globalThis[field];
    return typeof g === "function" ? g.bind(globalThis) : undefined;
  }
  const owner = globalThis[module];
  const fn = owner && owner[field];
  return typeof fn === "function" ? fn.bind(owner) : undefined;
}

// Slot tags for the dynamic-`js` argument buffer. One argument = one 16-byte slot laid out as
// `[tag: i32][aux: i32][payload: 8 bytes]`. Must match `src/mir/emit/types.rs::js_slot`.
export const JS_SLOT = {
  NULL: 0, INT: 1, LONG: 2, DOUBLE: 3, BOOL: 4, STRING: 5, JS: 6, FUNC: 7, ARRAY: 8,
  ISIZE: 9, USIZE: 10,
};
// Maps an array element's slot tag (the `aux` word of an ARRAY slot) to the Dream element-type name
// understood by `readArray`.
export const JS_ARRAY_ELEM = {
  [JS_SLOT.INT]: "int", [JS_SLOT.LONG]: "long", [JS_SLOT.DOUBLE]: "double",
  [JS_SLOT.BOOL]: "bool", [JS_SLOT.STRING]: "string", [JS_SLOT.JS]: "js",
  [JS_SLOT.ISIZE]: "isize", [JS_SLOT.USIZE]: "usize",
};

/**
 * Decodes `argc` tagged argument slots starting at `ptr` (in the instance's linear memory) into an
 * array of live JS values. Primitives are read in place, strings/arrays are materialized, `js`
 * handles are dereferenced, and Dream funcrefs are wrapped as identity-stable JS callables. This is
 * the read side of the shadow-stack marshaling emitted by `Emitter::emit_js_call`.
 */
export function decodeJsSlots(inst, ptr, argc) {
  const dv = inst.view;
  const out = new Array(argc);
  for (let i = 0; i < argc; i++) {
    const base = ptr + i * 16;
    const tag = dv.getInt32(base, true);
    const aux = dv.getInt32(base + 4, true);
    const p = base + 8;
    switch (tag) {
      case JS_SLOT.NULL: out[i] = null; break;
      case JS_SLOT.INT: out[i] = dv.getInt32(p, true); break;
      case JS_SLOT.ISIZE: out[i] = dv.getInt32(p, true); break;
      case JS_SLOT.USIZE: out[i] = dv.getUint32(p, true); break;
      case JS_SLOT.LONG: out[i] = dv.getBigInt64(p, true); break;
      case JS_SLOT.DOUBLE: out[i] = dv.getFloat64(p, true); break;
      case JS_SLOT.BOOL: out[i] = dv.getInt32(p, true) !== 0; break;
      case JS_SLOT.STRING: out[i] = inst.readString(dv.getInt32(p, true)); break;
      case JS_SLOT.JS: out[i] = inst.derefHandle(dv.getInt32(p, true)); break;
      case JS_SLOT.FUNC: {
        // `aux` is the callback's parameter count; each parameter is passed as a `js` handle and the
        // result is discarded (`void`), so reconstruct `fun(js, js, …): void` of the right arity.
        const arity = aux > 0 ? aux : 0;
        const sig = `fun(${Array(arity).fill("js").join(",")}):void`;
        out[i] = inst.callback(dv.getInt32(p, true), sig);
        break;
      }
      case JS_SLOT.ARRAY: {
        const arrPtr = dv.getInt32(p, true);
        const elem = JS_ARRAY_ELEM[aux] || "int";
        out[i] = elem === "js"
          ? inst.readArray(arrPtr, "int").map((h) => inst.derefHandle(h))
          : inst.readArray(arrPtr, elem);
        break;
      }
      default: out[i] = null;
    }
  }
  return out;
}
