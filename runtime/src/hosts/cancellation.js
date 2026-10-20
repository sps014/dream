import { wrapAsyncImport } from "../async.js";

export function makeCancellationHost(getInstance) {
  const waiters = new Map();
  const shared = buffer => typeof SharedArrayBuffer !== "undefined" && buffer instanceof SharedArrayBuffer;
  return {
    asyncHostCancel: future => getInstance()._hostCancellations?.get(future)?.(),
    cancellationSubscribe: (future, registration) => {
      const inst = getInstance();
      // Attach synchronously: the C owner can unregister and recycle this word before a microtask.
      const word = new Int32Array(inst.memory.buffer, registration, 1);
      const record = { future, resolve: () => {} };
      let pending;
      if (shared(word.buffer)) {
        const wait = Atomics.waitAsync(word, 0, 0);
        if (wait.async) {
          waiters.set(registration, record);
          // Node does not count an Atomics.waitAsync waiter as event-loop work, so a wake posted
          // by another thread can arrive after the loop has drained and exited.
          const keepAlive = setInterval(() => {}, 0x7fffffff);
          pending = wait.value.then(() => {
            clearInterval(keepAlive);
            if (waiters.get(registration) === record) waiters.delete(registration);
            return 0;
          });
        } else pending = Promise.resolve(0);
      } else if (word[0] !== 0) pending = Promise.resolve(0);
      else pending = new Promise(resolve => { record.resolve = resolve; waiters.set(registration, record); });
      // Notification is final; no callback reads the registration after its C owner frees it.
      wrapAsyncImport(getInstance, () => pending, { params: [], result: "int" })(future);
    },
    cancellationNotify: registration => {
      const inst = getInstance();
      const word = new Int32Array(inst.memory.buffer, registration, 1);
      if (shared(word.buffer)) Atomics.notify(word, 0);
      const waiter = waiters.get(registration);
      waiters.delete(registration);
      if (waiter) {
        // The C owner has completed/detached the subscription, so its pin can settle now.
        inst._hostCompletions?.get(waiter.future)?.(0);
        waiter.resolve(0);
      }
    },
  };
}
