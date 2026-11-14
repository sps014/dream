import assert from "node:assert/strict";
import test from "node:test";
import childProcess from "node:child_process";
import { setNodeChildProcess } from "../runtime/src/platform.js";
import { makeProcessHost } from "../runtime/src/hosts/process_service.js";
setNodeChildProcess(childProcess);
const decode = bytes => new TextDecoder().decode(Uint8Array.from(bytes));

async function finish(host, id) {
  await host.processCallWait(id);
  const code = host.processCallCode(id);
  const result = code ? { code } : { value: host.processCallValue(id), bytes: host.processCallBytes(id) };
  host.processCallRelease(id);
  return result;
}

async function spawn(host, code, env = []) {
  const launched = await finish(host, host.processSpawn(process.execPath, ["-e", code], "", env, false));
  assert.equal(launched.code, undefined);
  return Number(launched.value);
}

async function readAll(host, child, stream) {
  let text = "";
  for (;;) {
    const read = await finish(host, host.processRead(child, stream, 65536));
    assert.equal(read.code, undefined);
    if (!read.bytes.length) return text;
    text += decode(read.bytes);
  }
}

test("arguments and environment overrides reach the child unchanged", async () => {
  const host = makeProcessHost();
  const args = ["", "two words", "a\nb", "こんにちは"];
  const launched = await finish(host, host.processSpawn(process.execPath, ["-e", "console.log(JSON.stringify(process.argv.slice(1))); console.error(process.env.DREAM_PROCESS_TEST)", ...args], "", ["DREAM_PROCESS_TEST", "ready"], false));
  const child = Number(launched.value);
  assert.ok((await readAll(host, child, 0)).includes(JSON.stringify(args)));
  assert.equal(await readAll(host, child, 1), "ready\n");
  assert.equal((await finish(host, host.processWait(child))).value, 0n);
  host.processRelease(child);
});

test("pipes stream, stdin EOF ends the child, and release invalidates the handle", async () => {
  const host = makeProcessHost();
  const child = await spawn(host, "process.stdin.pipe(process.stdout)");
  assert.equal((await finish(host, host.processWrite(child, [104, 105]))).value, 2n);
  assert.equal(decode((await finish(host, host.processRead(child, 0, 2))).bytes), "hi");
  host.processCloseStdin(child);
  assert.equal((await finish(host, host.processWait(child))).value, 0n);
  host.processRelease(child);
  assert.equal((await finish(host, host.processRead(child, 0, 1))).code, "ECLOSED");
});

test("cancelled reads keep their bytes and kill ends pending reads", async () => {
  const host = makeProcessHost();
  const child = await spawn(host, "setInterval(()=>{},1000)");
  const read = host.processRead(child, 0, 16);
  host.processCallCancel(read);
  assert.equal((await finish(host, read)).code, "ECANCELLED");
  const pending = host.processRead(child, 0, 16);
  host.processKill(child);
  assert.equal((await finish(host, pending)).code, "ECLOSED");
  assert.ok((await finish(host, host.processWait(child))).value < 0n);
  host.processRelease(child);
});

test("launch failures and invalid text are reported", async () => {
  const host = makeProcessHost();
  assert.equal((await finish(host, host.processSpawn("dream-missing-binary", [], "", [], false))).code, "ESPAWN");
  assert.equal((await finish(host, host.processSpawn(process.execPath, ["a\0b"], "", [], false))).code, "EINVAL");
  assert.equal((await finish(host, host.processSpawn(process.execPath, [], "", ["A=B", "x"], false))).code, "EINVAL");
});
