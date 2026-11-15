import assert from "node:assert/strict";
import test from "node:test";
import childProcess from "node:child_process";
import { setNodeChildProcess } from "../runtime/src/platform.js";
import { makeProcessHost } from "../runtime/src/hosts/process_service.js";
import { encodeProcessArgs } from "../runtime/src/hosts/process_wire.js";
import { WireReader } from "../runtime/src/wire.js";
setNodeChildProcess(childProcess);
const decode = bytes => new TextDecoder().decode(Uint8Array.from(bytes));
async function complete(host, id) {
  await host.processOperationWait(id);
  const bytes = host.processOperationResult(id);
  host.processOperationRelease(id);
  assert.notEqual(bytes[0], 0);
  return bytes[0] === 1 ? { value: bytes.slice(1) } : { error: { code: new WireReader(bytes.slice(1)).text() } };
}
function start(host, code, options = {}) {
  return host.processStart(JSON.stringify({ action: "run", command: process.execPath, options }), encodeProcessArgs(["-e", code]));
}
test("run preserves argument boundaries, environment overrides, and asynchronous progress", async () => {
  const host = makeProcessHost();
  const args = ["", "two words", "a\nb", "こんにちは"];
  const id = host.processStart(JSON.stringify({ action: "run", command: process.execPath, options: { env: { DREAM_PROCESS_TEST: "ready" } } }), encodeProcessArgs(["-e", "console.log(JSON.stringify(process.argv.slice(1))); console.error(process.env.DREAM_PROCESS_TEST)", ...args]));
  let progressed = false;
  setImmediate(() => { progressed = true; });
  const result = await complete(host, id);
  assert.equal(result.error, undefined);
  assert.ok(progressed);
  assert.ok(decode(result.value).includes(JSON.stringify(args)));
  assert.ok(decode(result.value).endsWith("ready\n"));
});
test("run concurrently feeds input and drains saturated stdout and stderr", async () => {
  const host = makeProcessHost();
  const input = new Array(200000).fill(120);
  const result = await complete(host, start(host, "process.stdout.write('a'.repeat(200000)); process.stderr.write('b'.repeat(200000)); process.stdin.resume(); process.stdin.on('end',()=>console.log('done'))", { input, timeout_ms: 5000 }));
  assert.equal(result.error, undefined);
  assert.ok(result.value.length > 400000);
});
test("deadline, cancellation, and output limits settle before returning", async () => {
  const host = makeProcessHost();
  const deadline = await complete(host, start(host, "setInterval(()=>{},1000)", { timeout_ms: 50 }));
  assert.equal(deadline.error.code, "ETIMEOUT");
  const id = start(host, "setInterval(()=>{},1000)");
  const abandoned = host.processOperationWait(id);
  await new Promise(resolve => setTimeout(resolve, 50));
  host.processOperationCancel(id);
  const cancelled = await complete(host, id);
  await abandoned;
  assert.equal(cancelled.error.code, "ECANCELLED");
  const limited = await complete(host, start(host, "process.stdout.write('x'.repeat(1000000))", { max_output_bytes: 1000 }));
  assert.equal(limited.error.code, "ELIMIT");
});
test("spawned pipes stream, stdin EOF completes the child, and close invalidates handles", async () => {
  const host = makeProcessHost();
  const started = host.processStart(JSON.stringify({ action: "spawn", command: process.execPath, options: {} }), encodeProcessArgs(["-e", "process.stdin.pipe(process.stdout)"]));
  await host.processOperationWait(started);
  const wire = host.processOperationResult(started);
  assert.equal(wire[0], 1);
  const handle = Number(decode(wire.slice(1)));
  host.processOperationClaim(started);
  host.processOperationRelease(started);
  const operation = (action, extra = {}, bytes = []) => host.processStart(JSON.stringify({ action, handle, ...extra }), bytes);
  assert.equal(decode((await complete(host, operation("write", {}, [104, 105]))).value), "2");
  assert.equal(decode((await complete(host, operation("read", { stream: 0, max_bytes: 2 }))).value), "hi");
  assert.equal((await complete(host, operation("close_stdin"))).error, undefined);
  assert.equal(decode((await complete(host, operation("wait"))).value), "0");
  assert.equal((await complete(host, operation("close"))).error, undefined);
  assert.equal((await complete(host, operation("read", { stream: 0, max_bytes: 1 }))).error.code, "ECLOSED");
});

test("malformed argument wires and invalid capture bounds agree with native validation", async () => {
  const host = makeProcessHost();
  const id = host.processStart(JSON.stringify({ action: "run", command: process.execPath, options: {} }), [255, 255, 255, 255]);
  assert.equal((await complete(host, id)).error.code, "EINVAL");
  assert.equal((await complete(host, start(host, "", { max_output_bytes: 2147483647 }))).error.code, "EINVAL");
});
