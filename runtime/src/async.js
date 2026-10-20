import { TAGS, stripSuffix } from "./core.js";
import { marshalArgs, marshalResult } from "./marshal.js";

/**
 * Boxes a 64-bit numeric result into a Dream heap block matching `$box_long`/`$box_ulong` in
 * the C guest runtime. Returns the block's data pointer, which is what `F_RESULT` stores.
 * Called from `wrapAsyncImport` for `extern async fun ...: long | ulong` returns; a synchronous
 * `long`-returning extern uses the WASM i64 signature directly and needs no boxing.
 */
export function boxLong64(inst, value, tag) {
  if (typeof inst.exports.malloc !== "function") {
    throw new Error("module does not export `malloc`; cannot box a `long` async result");
  }
  const ptr = inst.exports.malloc(8, tag);
  const big = typeof value === "bigint" ? value : BigInt(value == null ? 0 : value);
  if (tag === TAGS.ULONG) {
    inst.view.setBigUint64(ptr, big < 0n ? 0n : big, true);
  } else {
    inst.view.setBigInt64(ptr, big, true);
  }
  return ptr;
}

const runLoopPumps = new WeakMap();

/**
 * Drain `__dream_run_loop` without nesting. Browsers flush microtasks at wasm→JS import
 * boundaries, so a Promise `then` can run while poll is still on the stack. Calling the
 * scheduler there re-enters poll until `$malloc` OOBs. Key state by the Dream instance
 * (the WASM exports object is not a reliable WeakMap key) and always hop to a microtask
 * so `run_loop` never starts from inside an import.
 */
export function pumpRunLoop(inst) {
  let s = runLoopPumps.get(inst);
  if (!s) {
    s = { running: false, queued: false };
    runLoopPumps.set(inst, s);
  }
  s.queued = true;
  if (s.running) return;
  s.running = true;
  const drain = () => {
    try {
      while (s.queued) {
        s.queued = false;
        inst.exports.__dream_run_loop();
      }
    } catch (error) {
      inst._hostError = error;
    } finally {
      s.running = false;
      if (s.queued) {
        s.running = true;
        queueMicrotask(drain);
      }
    }
  };
  queueMicrotask(drain);
}

/**
 * Wraps an `extern async` import. The guest allocates a lazy Future for each call and only
 * invokes the import from that future's first poll, passing the future as the leading argument;
 * the wrapper marshals the remaining args, kicks off the host Promise, and settles the
 * guest-provided future (re-pumping the scheduler) when it resolves. This is the only place the
 * JS `.then` bridge lives - Dream source never sees a Promise.
 */
export function wrapAsyncImport(getInstance, fn, signature) {
  const params = signature ? signature.params : null;
  const result = signature ? signature.result : null;
  // `long`/`ulong` async returns cross the future boundary as boxed heap pointers because
  // `F_RESULT` is a single i32; a bare BigInt would fail wasm's i32 coercion.
  const resultBase = typeof result === "string" ? stripSuffix(result) : result;
  const boxTag = resultBase === "long"
    ? TAGS.LONG
    : resultBase === "ulong"
      ? TAGS.ULONG
      : null;

  return startAsyncImport(getInstance, fn,
    (inst, args) => marshalArgs(inst, params, args),
    (inst, value) => boxTag != null ? boxLong64(inst, value, boxTag) : marshalResult(inst, result, value));
}

export function startAsyncImport(getInstance, fn, convertArgs, convertResult) {
  return (future, ...rawArgs) => {
    const inst = getInstance();
    const exports = inst.exports;
    const args = convertArgs(inst, rawArgs);
    exports.__dream_host_pin(future);
    const cancellation = { requested: false, cancel: null };
    inst._hostCancellations ??= new Map();
    inst._hostCancellations.set(future, () => {
      cancellation.requested = true;
      cancellation.cancel?.();
      // Only built-ins that synchronously stop their work may release the frame immediately.
      if (fn.cancelSynchronously === true) settle(null);
    });
    let settled = false;
    const settle = (rawResult) => {
      if (settled) return;
      settled = true;
      inst._hostCompletions?.delete(future);
      inst._hostPending--;
      inst._hostCancellations.delete(future);
      try {
        if (exports.__dream_host_status(future) === 0) {
          const marshaled = convertResult(inst, rawResult);
          exports.__dream_resolve(future, marshaled);
          pumpRunLoop(inst);
        }
      } finally {
        exports.__dream_host_unpin(future);
      }
    };
    inst._hostCompletions ??= new Map();
    inst._hostCompletions.set(future, settle);
    inst._hostPending++;
    const pending = Promise.resolve().then(() => {
      if (settled) return null;
      const work = fn(...args);
      if (typeof work?.cancel === "function") {
        cancellation.cancel = () => work.cancel();
        if (cancellation.requested) cancellation.cancel();
      }
      return work;
    }).then(
      (value) => settle(value),
      (err) => {
        // A rejected Promise has no Dream-level error channel yet; settle the future with a
        // zero/null result (a `null` `js` handle for a `js` result) so the scheduler is not left
        // hanging, and surface the reason on the console for diagnosis.
        console.error("Dream: awaited JS promise rejected:", err);
        settle(null);
      },
    );
    inst._hostSettlements.add(pending);
    pending.then(
      () => inst._hostSettlements.delete(pending),
      (error) => {
        inst._hostSettlements.delete(pending);
        inst._hostError = error;
        console.error("Dream: async host settlement failed:", error);
      },
    );
  };
}
