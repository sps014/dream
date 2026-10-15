import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { makeCancellationHost } from "../runtime/src/hosts/cancellation.js";

function fixture(shared = false) {
  const settled = [];
  let pins = 0;
  let cancelled = false;
  const inst = {
    memory: new WebAssembly.Memory({ initial: 1, maximum: 2, shared }),
    _hostPending: 0,
    _hostSettlements: new Set(),
    exports: {
      __dream_host_pin: () => pins++,
      __dream_host_unpin: () => pins--,
      __dream_host_status: () => cancelled ? 2 : 0,
      __dream_resolve: (future, value) => settled.push([future, value]),
      __dream_run_loop: () => {},
    },
  };
  const host = makeCancellationHost(() => inst);
  return { inst, host, settled, get pins() { return pins; }, cancel: () => { cancelled = true; } };
}

test("unshared cancellation detaches synchronously before a registration is recycled", async () => {
  const f = fixture();
  f.host.cancellationSubscribe(7, 16);
  new Int32Array(f.inst.memory.buffer)[4] = 2;
  f.host.cancellationNotify(16);
  // Reusing the freed word cannot affect a notification already delivered.
  new Int32Array(f.inst.memory.buffer)[4] = 0;
  await Promise.all([...f.inst._hostSettlements]);
  assert.deepEqual(f.settled, [[7, 0]]);
  assert.equal(f.pins, 0);
  assert.equal(f.inst._hostPending, 0);
});

test("cancelled frames release host pins without resuming guest code", async () => {
  const f = fixture();
  f.host.cancellationSubscribe(8, 16);
  f.cancel();
  new Int32Array(f.inst.memory.buffer)[4] = 1;
  f.host.cancellationNotify(16);
  await Promise.all([...f.inst._hostSettlements]);
  assert.deepEqual(f.settled, []);
  assert.equal(f.pins, 0);
  assert.equal(f.inst._hostPending, 0);
});

test("shared cancellation wakes across workers and memory growth without polling", async () => {
  const f = fixture(true);
  f.host.cancellationSubscribe(9, 16);
  f.inst.memory.grow(1);
  const worker = new Worker(`
    const { workerData } = require('node:worker_threads');
    const word = new Int32Array(workerData, 16, 1);
    Atomics.store(word, 0, 1);
    Atomics.notify(word, 0);
  `, { eval: true, workerData: f.inst.memory.buffer });
  await Promise.all([...f.inst._hostSettlements]);
  await new Promise((resolve, reject) => { worker.on("exit", code => code ? reject(new Error(String(code))) : resolve()); worker.on("error", reject); });
  assert.deepEqual(f.settled, [[9, 0]]);
  assert.equal(f.pins, 0);
  assert.equal(f.inst._hostPending, 0);
});
