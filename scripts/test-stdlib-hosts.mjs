import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { makeFsHost } from "../runtime/src/hosts/fs.js";
import { makeConsoleProcessHost } from "../runtime/src/hosts/console_process.js";
import { setNodeFs, setNodeChildProcess } from "../runtime/src/platform.js";

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
  const args = ["", "two words", "first\nsecond", "こんにちは", "\uFEFFleading", '"quoted"\\tail'];
  let called = 0;
  setNodeChildProcess({ spawnSync(command, actual) {
    called++;
    assert.equal(command, "example");
    assert.deepEqual(actual, args);
    return { status: 0, stdout: Buffer.from("ok"), stderr: Buffer.alloc(0) };
  }});
  try {
    const host = makeConsoleProcessHost();
    assert.equal(host.processRun("example", argumentWire(args), "").toString(), "0\n2\nok");
    const valid = argumentWire(args);
    for (let length = 0; length < valid.length; length++) {
      assert.ok(host.processRun("example", valid.subarray(0, length), "").toString().startsWith("-1\n"));
    }
    assert.ok(host.processRun("example", argumentWire(["a\0b"]), "").toString().startsWith("-1\n"));
    assert.equal(called, 1);
  } finally { setNodeChildProcess(null); }
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

test("URL host preserves normalized components and ordered decoded parameters", async () => {
  const { makeNetHost } = await import("../runtime/src/hosts/net.js");
  const host = makeNetHost();
  assert.deepEqual(host.netUrlParse("HTTPS://ExAmPle.Com:443/a/../b?q=a+b&q=%2B#", ""),
    ["https://example.com/b", "https", "example.com", "/b", "#", "?q=a+b&q=%2B", "q", "a b", "q", "+"]);
  assert.equal(host.netUrlParse("../users", "https://example.com/v1/nested/")[0], "https://example.com/v1/users");
  assert.equal(host.netUrlParse("https://bücher.example/こんにちは", "")[2], "xn--bcher-kva.example");
  assert.equal(host.netUrlParse("https://[::1]:443/", "")[2], "[::1]");
  assert.deepEqual(host.netUrlParse("relative", ""), []);
  assert.deepEqual(host.netUrlParse("https://[bad", ""), []);
  assert.deepEqual(host.netUrlParse("https://example.com/?#", "").slice(4, 6), ["#", "?"]);
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
  const { wrapAsyncImport } = await import("../runtime/src/marshal.js");
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
