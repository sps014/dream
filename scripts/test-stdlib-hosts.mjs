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
