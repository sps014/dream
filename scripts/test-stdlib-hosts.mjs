import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { makeFsHost } from "../runtime/src/hosts/fs.js";
import { makeConsoleProcessHost } from "../runtime/src/hosts/console_process.js";
import { splitProcessArgs } from "../runtime/src/hosts/process_wire.js";
import { setNodeFs, setNodeChildProcess } from "../runtime/src/platform.js";
import { makeSessionTable, makeWorkerModule } from "../runtime/src/workers.js";

/** Records messages; a test drives replies through `emit` and can make posting throw. */
function fakeWorkers() {
  const spawned = [];
  class FakeWorker {
    constructor() {
      this.listeners = {};
      this.posted = [];
      this.throwOn = null;
      spawned.push(this);
    }
    on(event, listener) { this.listeners[event] = listener; }
    emit(event, value) { this.listeners[event]?.(value); }
    postMessage(message) {
      if (message.t === this.throwOn) throw new Error(`cannot post ${message.t}`);
      this.posted.push(message);
    }
    terminate() { return Promise.resolve(0); }
    ref() {}
    unref() {}
  }
  const host = makeWorkerModule(new Uint8Array(), null, () => null, 0, () => null, FakeWorker);
  return { host, spawned };
}

test("a crashed worker settles its dispatches and reports a typed failure", async () => {
  const { host, spawned } = fakeWorkers();
  const id = host.workerSpawn(false);
  spawned[0].emit("message", { t: "ready", stack: 0 });
  const first = host.workerDispatch(id, 1, 0, "");
  const queued = host.workerDispatch(id, 1, 0, "");
  assert.equal(host.workerFailure(id), "");
  spawned[0].emit("message", { t: "error", phase: "dispatch", message: "RuntimeError: unreachable" });
  assert.equal(await first, "");
  assert.equal(await queued, "");
  assert.match(host.workerFailure(id), /unreachable/);
  assert.equal(await host.workerDispatch(id, 1, 0, ""), "");
  host.workerTerminate(id);
  assert.equal(host.workerFailure(id), "");
  await host.__drainWorkers();
});

test("a dispatched worker that never boots fails after the boot timeout", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const { host } = fakeWorkers();
  const id = host.workerSpawn(false);
  const pending = host.workerDispatch(id, 1, 0, "");
  t.mock.timers.tick(9999);
  assert.equal(host.workerFailure(id), "");
  t.mock.timers.tick(1);
  assert.equal(await pending, "");
  assert.match(host.workerFailure(id), /bootstrap timed out/);
  host.workerTerminate(id);
});

test("a postMessage that throws fails the worker instead of escaping", async () => {
  const { host, spawned } = fakeWorkers();
  const id = host.workerSpawn(false);
  spawned[0].throwOn = "dispatch";
  const pending = host.workerDispatch(id, 1, 0, "");
  spawned[0].emit("message", { t: "ready", stack: 0 });
  assert.equal(await pending, "");
  assert.match(host.workerFailure(id), /cannot post dispatch/);
  host.workerTerminate(id);

  const fresh = host.workerSpawn(false);
  spawned[1].emit("message", { t: "ready", stack: 0 });
  spawned[1].throwOn = "term";
  host.workerTerminate(fresh);
  await host.__drainWorkers();
});

test("worker session handles are generational and never run out", () => {
  const table = makeSessionTable();
  const first = table.insert("a");
  assert.equal(table.remove(first), "a");
  assert.equal(table.get(first), undefined);
  assert.equal(table.remove(first), undefined);
  const second = table.insert("b");
  assert.notEqual(second, first);
  assert.equal(table.get(second), "b");
  table.remove(second);

  const slot = (id) => id & 0x7fffff;
  const batch = [table.insert(0), table.insert(1), table.insert(2)];
  batch.forEach((id) => table.remove(id));
  const reused = [table.insert(0), table.insert(1), table.insert(2)];
  assert.deepEqual(reused.map(slot), batch.map(slot));
  reused.forEach((id) => table.remove(id));

  const keep = Array.from({ length: 64 }, (_, i) => table.insert(i));
  let wrapped = false;
  const previous = new Map();
  for (let i = 0; i < 1_000_000; i++) {
    const id = table.insert(i);
    const generation = id >>> 23;
    assert.ok(id > 0 && generation >= 1);
    if (previous.get(slot(id)) === 255 && generation === 1) wrapped = true;
    previous.set(slot(id), generation);
    assert.equal(table.remove(id), i);
  }
  assert.ok(wrapped);
  keep.forEach((id, i) => assert.equal(table.get(id), i));
});

for (const backend of ["memory", "node"]) {
  test(`${backend} handles distinguish failures, EOF, and closure`, () => {
    setNodeFs(backend === "node" ? fs : null);
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), "dream-io-"));
    const name = path.join(directory, "data");
    const host = makeFsHost();
    try {
      const handle = host.fileOpen(name, "w+");
      assert.ok(handle >= 0);
      assert.deepEqual(host.fileHandleRead(handle, -1), [1]);
      assert.equal(host.fileHandleWrite(handle, [65, 66]), 2n);
      assert.equal(host.fileHandleLength(handle), 2n);
      assert.equal(host.fileHandleSeek(handle, 0n), 0);
      assert.deepEqual(host.fileHandleRead(handle, 3), [0, 65, 66]);
      assert.deepEqual(host.fileHandleRead(handle, 3), [0]);
      assert.equal(host.fileHandleSeek(handle, -1n), -1);
      assert.equal(host.fileHandleTell(handle), 2n);
      assert.equal(host.fileHandleClose(handle), 0);
      assert.deepEqual(host.fileHandleRead(handle, 1), [1]);
      const readOnly = host.fileOpen(name, "r");
      assert.equal(host.fileHandleWrite(readOnly, [1]), -1n);
      assert.equal(host.fileHandleClose(readOnly), 0);
      const truncate = host.fileOpen(name, "w");
      assert.deepEqual(host.fileHandleRead(truncate, 1), [1]);
      assert.equal(host.fileHandleLength(truncate), 0n);
      host.fileHandleClose(truncate);
    } finally {
      fs.rmSync(directory, { recursive: true, force: true });
      setNodeFs(null);
    }
  });
}

function argumentWire(args) {
  const parts = [];
  const length = (n) => { const bytes = Buffer.alloc(4); bytes.writeUInt32LE(n); return bytes; };
  parts.push(length(args.length));
  for (const argument of args) {
    const bytes = Buffer.from(argument);
    parts.push(length(bytes.length), bytes);
  }
  return Buffer.concat(parts);
}

test("process arguments preserve byte boundaries and reject malformed input", () => {
  const args = ["", "two words", "first\nsecond", "こんにちは", "\uFEFFleading", '\"quoted\"\\tail'];
  assert.deepEqual(splitProcessArgs(argumentWire(args)), args);
  const valid = argumentWire(args);
  for (let length = 0; length < valid.length; length++) assert.throws(() => splitProcessArgs(valid.subarray(0, length)));
  assert.throws(() => splitProcessArgs(argumentWire(["a\0b"])));
});


test("System.args host preserves empty, multiline, and Unicode arguments", () => {
  const original = process.argv;
  const args = ["", "two words", "first\nsecond", "こんにちは"];
  try {
    process.argv = [original[0], "example", ...args];
    const host = makeConsoleProcessHost();
    assert.deepEqual(Buffer.from(host.processArgs()), argumentWire(args));
  } finally { process.argv = original; }
});

test("domain host maps internationalized names with UTS #46", async () => {
  const { makeNetHost } = await import("../runtime/src/hosts/net.js");
  const host = makeNetHost();
  assert.equal(host.netDomainToAscii("b\u00fccher.example"), "xn--bcher-kva.example");
  assert.equal(host.netDomainToAscii("XN--BCHER-KVA.example"), "xn--bcher-kva.example");
  assert.equal(host.netDomainToAscii("xn--a"), "");
});

test("Node file operations keep buffers and descriptors alive through abandonment", async () => {
  const callbacks = [];
  let closed = 0;
  const realFs = fs;
  setNodeFs({
    read(fd, bytes, offset, count, position, callback) {
      callbacks.push(() => { bytes.set([65, 66]); callback(null, 2); });
    },
    write(fd, bytes, offset, count, position, callback) {
      callbacks.push(() => { assert.deepEqual(Array.from(bytes), [67, 68]); callback(null, 2); });
    },
    closeSync() { closed++; },
    open(path, mode, callback) { callbacks.push(() => callback(null, 41)); },
  });
  const host = makeFsHost();
  try {
    const opening = host.fileOperationStart(0, -1, "fake", "r+", 0, [], 0n);
    assert.equal(host.fileOperationDone(opening), 0);
    callbacks.shift()();
    const file = Number(host.fileOperationResult(opening));
    host.fileOperationRelease(opening);
    const reading = host.fileOperationStart(1, file, "", "", 2, [], 0n);
    const waiting = host.fileOperationWait(reading);
    assert.equal(host.fileHandleClose(file), 0);
    assert.equal(closed, 0);
    host.fileOperationRelease(reading);
    callbacks.shift()();
    await waiting;
    assert.equal(closed, 1);
    const orphan = host.fileOperationStart(0, -1, "fake", "r+", 0, [], 0n);
    host.fileOperationRelease(orphan);
    callbacks.shift()();
    assert.equal(closed, 2);
    const again = host.fileOperationStart(0, -1, "fake", "r+", 0, [], 0n);
    callbacks.shift()();
    const writeFile = Number(host.fileOperationResult(again));
    host.fileOperationRelease(again);
    const input = [67, 68];
    const writing = host.fileOperationStart(2, writeFile, "", "", 2, input, 0n);
    input[0] = 0;
    callbacks.shift()();
    assert.equal(host.fileOperationResult(writing), 2n);
    host.fileOperationRelease(writing);
    host.fileHandleClose(writeFile);
    for (let i = 0; i < 132; i++) {
      const pending = host.fileOperationStart(0, -1, "fake", "r", 0, [], 0n);
      assert.ok(pending > 0);
      host.fileOperationRelease(pending);
    }
    assert.equal(host.fileOperationStart(0, -1, "fake", "r", 0, [], 0n), 0);
    while (callbacks.length) callbacks.shift()();
    assert.equal(closed, 135);
  } finally { setNodeFs(realFs); }
});

test("late async JS settlement skips cancelled frames and releases its pin", async () => {
  const { wrapAsyncImport } = await import("../runtime/src/async.js");
  let refs = 1;
  let status = 0;
  let allocations = 0;
  let resolved = 0;
  let finish;
  const instance = {
    _hostPending: 0, _hostSettlements: new Set(),
    writeString() { allocations++; return 42; },
    exports: {
      __dream_host_pin() { refs++; },
      __dream_host_status() { assert.ok(refs > 0); return status; },
      __dream_host_unpin() { refs--; },
      __dream_resolve() { resolved++; },
    },
  };
  const call = wrapAsyncImport(() => instance, () => new Promise((resolve) => { finish = resolve; }), { params: [], result: "string" });
  call(7);
  await Promise.resolve();
  status = 2;
  refs--;
  assert.equal(refs, 1);
  finish("discarded");
  await Promise.all(instance._hostSettlements);
  assert.equal(refs, 0);
  assert.equal(instance._hostPending, 0);
  assert.equal(allocations, 0);
  assert.equal(resolved, 0);
});
