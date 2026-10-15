import { wrapAsyncImport } from "../marshal.js";

export function makeCancellationHost(getInstance) {
  const waiters = new Map();
  const shared = buffer => typeof SharedArrayBuffer !== "undefined" && buffer instanceof SharedArrayBuffer;
  return {
    cancellationSubscribe: (future, registration) => {
      const inst = getInstance();
      // Attach synchronously: the C owner can unregister and recycle this word before a microtask.
      const word = new Int32Array(inst.memory.buffer, registration, 1);
      let pending;
      if (shared(word.buffer)) {
        const wait = Atomics.waitAsync(word, 0, 0);
        pending = wait.async ? wait.value.then(() => 0) : Promise.resolve(0);
      } else if (word[0] !== 0) pending = Promise.resolve(0);
      else pending = new Promise(resolve => waiters.set(registration, resolve));
      // Notification is final; no callback reads the registration after its C owner frees it.
      wrapAsyncImport(getInstance, () => pending, { params: [], result: "int" })(future);
    },
    cancellationNotify: registration => {
      const inst = getInstance();
      const word = new Int32Array(inst.memory.buffer, registration, 1);
      if (shared(word.buffer)) Atomics.notify(word, 0);
      else {
        const resolve = waiters.get(registration);
        waiters.delete(registration);
        resolve?.(0);
      }
    },
  };
}
