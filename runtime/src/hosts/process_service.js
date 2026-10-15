import { isNode, getNodeChildProcess } from "../platform.js";
import { splitProcessArgs } from "./process_wire.js";

const processChildren = new Map();
const processOperations = new Map();
let nextProcessServiceId = 1;
function processFailure(code, message) { return { code, message }; }
function processReason(signal) { return signal.reason?.code ? signal.reason : processFailure("ECANCELLED", "process operation cancelled"); }
function processRequire(id) {
  const child = processChildren.get(id);
  if (!child) throw processFailure("ECLOSED", "child process is closed");
  return child;
}
function processOptions(options = {}) {
  const timeout = options.timeout_ms ?? 0;
  const maximum = options.max_output_bytes || 8388608;
  if (!Number.isInteger(timeout) || timeout < 0 || timeout > 2147483647 || !Number.isInteger(maximum) || maximum < 0 || maximum > 1073741791) throw processFailure("EINVAL", "invalid process options");
  const environment = options.clear_env ? {} : { ...process.env };
  for (const [key, value] of Object.entries(options.env || {})) {
    if (!key || /[=\0]/.test(key) || typeof value !== "string" || value.includes("\0")) throw processFailure("EINVAL", "invalid environment override");
    Object.defineProperty(environment, key, { value, enumerable: true, writable: true, configurable: true });
  }
  return { cwd: options.cwd || undefined, env: environment, timeout, maximum, input: options.input || [] };
}
async function processStop(state) {
  if (!state.closed) state.child.kill("SIGKILL");
  await state.exited;
  processChildren.delete(state.id);
}
function processAbortable(signal, setup) {
  if (signal.aborted) return Promise.reject(processReason(signal));
  return new Promise((resolve, reject) => {
    let cleanup = () => {};
    const finish = (error, value) => { signal.removeEventListener("abort", abort); cleanup(); error ? reject(error) : resolve(value); };
    const abort = () => finish(processReason(signal));
    signal.addEventListener("abort", abort, { once: true });
    cleanup = setup(value => finish(null, value), error => finish(error));
  });
}
async function processCreate(command, bytes, options, signal) {
  if (!isNode || !getNodeChildProcess()) throw processFailure("EUNSUPPORTED", "processes are unavailable in the browser");
  const settings = processOptions(options);
  let args;
  try { args = splitProcessArgs(bytes); } catch (_) { throw processFailure("EINVAL", "invalid process argument encoding"); }
  let child;
  try { child = getNodeChildProcess().spawn(command, args, { cwd: settings.cwd, env: settings.env, stdio: "pipe", windowsHide: true }); }
  catch (error) { throw processFailure(error?.code === "EINVAL" ? "EINVAL" : "ESPAWN", error.message || String(error)); }
  const state = { id: nextProcessServiceId++, child, closed: false, error: null, timer: null, settings };
  state.exited = new Promise(resolve => {
    child.once("error", error => { state.error = processFailure("ESPAWN", error.message); });
    child.once("close", code => { state.closed = true; clearTimeout(state.timer); resolve(code ?? -2); });
  });
  for (const pipe of [child.stdin, child.stdout, child.stderr]) pipe?.on("error", () => {});
  processChildren.set(state.id, state);
  try {
    await processAbortable(signal, (resolve, reject) => {
      const spawned = () => resolve();
      const error = error => reject(processFailure("ESPAWN", error.message));
      child.once("spawn", spawned); child.once("error", error);
      return () => { child.off("spawn", spawned); child.off("error", error); };
    });
    return state;
  } catch (error) { await processStop(state); throw error; }
}
async function processRead(state, stream, maximum, signal) {
  if (!Number.isInteger(maximum) || maximum < 0 || maximum > 16777216 || ![0, 1].includes(stream)) throw processFailure("EINVAL", "invalid process read");
  if (maximum === 0) return [];
  const pipe = stream === 0 ? state.child.stdout : state.child.stderr;
  while (true) {
    if (signal.aborted) throw processReason(signal);
    if (state.error) throw state.error;
    const available = pipe.readableLength;
    if (available) return pipe.read(Math.min(maximum, available));
    if (pipe.readableEnded || pipe.destroyed) return [];
    await processAbortable(signal, (resolve, reject) => {
      const ready = () => resolve();
      const error = error => reject(processFailure("EIO", error.message));
      pipe.once("readable", ready); pipe.once("end", ready); pipe.once("close", ready); pipe.once("error", error);
      return () => { pipe.off("readable", ready); pipe.off("end", ready); pipe.off("close", ready); pipe.off("error", error); };
    });
  }
}
async function processWrite(state, bytes, signal) {
  const pipe = state.child.stdin;
  if (pipe.destroyed || pipe.writableEnded) throw processFailure("ECLOSED", "stdin closed");
  await processAbortable(signal, (resolve, reject) => {
    pipe.write(Buffer.from(bytes), error => error ? reject(processFailure("EIO", error.message)) : resolve());
    return () => {};
  });
  return bytes.length;
}
async function processExit(state, signal) {
  const result = await processAbortable(signal, (resolve) => { state.exited.then(resolve); return () => {}; });
  if (state.error) throw state.error;
  return result;
}
async function processCollect(state, stream, limit, signal) {
  const chunks = [];
  let count = 0;
  while (true) {
    const chunk = await processRead(state, stream, 65536, signal);
    if (!chunk.length) return Buffer.concat(chunks, count);
    count += chunk.length;
    if (count > limit) throw processFailure("ELIMIT", "captured process output exceeds configured limit");
    chunks.push(Buffer.from(chunk));
  }
}
async function processExecute(request, bytes, op) {
  const signal = op.controller.signal;
  if (signal.aborted) throw processReason(signal);
  if (request.action === "run" || request.action === "spawn") {
    const state = await processCreate(request.command, bytes, request.options, signal);
    if (request.action === "spawn") {
      if (state.settings.timeout) state.timer = setTimeout(() => { state.error = processFailure("ETIMEOUT", "process deadline exceeded"); processStop(state).catch(() => {}); }, state.settings.timeout);
      return { bytes: new TextEncoder().encode(String(state.id)), child: state };
    }
    let timer;
    if (state.settings.timeout) timer = setTimeout(() => op.controller.abort(processFailure("ETIMEOUT", "process deadline exceeded")), state.settings.timeout);
    try {
      const input = async () => { if (state.settings.input.length) await processWrite(state, state.settings.input, signal); state.child.stdin.end(); };
      const [, stdout, stderr, code] = await Promise.all([input(), processCollect(state, 0, state.settings.maximum, signal), processCollect(state, 1, state.settings.maximum, signal), processExit(state, signal)]);
      return { bytes: Buffer.concat([Buffer.from(`${code}\n${stdout.length}\n`), stdout, stderr]) };
    } finally { clearTimeout(timer); await processStop(state); }
  }
  const state = processRequire(request.handle);
  switch (request.action) {
    case "read": return { bytes: await processRead(state, request.stream, request.max_bytes, signal) };
    case "write": return { bytes: new TextEncoder().encode(String(await processWrite(state, bytes, signal))) };
    case "close_stdin": state.child.stdin.end(); return { bytes: [] };
    case "wait": return { bytes: new TextEncoder().encode(String(await processExit(state, signal))) };
    case "close": await processStop(state); return { bytes: [] };
    default: throw processFailure("EINVAL", "invalid process action");
  }
}
export function makeProcessHost() {
  return {
    processStart: (request, bytes) => {
      if (processOperations.size >= 4096 || nextProcessServiceId >= 2147483647) return 0;
      const id = nextProcessServiceId++;
      const op = { controller: new AbortController(), result: null, released: false, waiters: [], claimed: false };
      processOperations.set(id, op);
      Promise.resolve().then(() => {
        if (!isNode) throw processFailure("EUNSUPPORTED", "processes are unavailable in the browser");
        let parsed; try { parsed = JSON.parse(request); } catch (_) { throw processFailure("EINVAL", "invalid process request"); }
        return processExecute(parsed, bytes, op);
      }).then(result => {
        if (op.released) { if (result.child) processStop(result.child).catch(() => {}); return; }
        op.result = result;
      }, error => { if (!op.released) op.result = { error: error?.code ? error : processFailure("EIO", error?.message || String(error)) }; }).finally(() => {
        for (const resolve of op.waiters.splice(0)) resolve(0);
      });
      return id;
    },
    processOperationWait: id => {
      const op = processOperations.get(id);
      if (!op || op.result || op.released) return Promise.resolve(0);
      return new Promise(resolve => op.waiters.push(resolve));
    },
    processOperationResult: id => {
      const op = processOperations.get(id);
      if (!op) return [2, ...new TextEncoder().encode(JSON.stringify(processFailure("ECLOSED", "process operation closed")))];
      if (!op.result) return [0];
      if (op.result.error) return [2, ...new TextEncoder().encode(JSON.stringify(op.result.error))];
      const result = new Uint8Array(op.result.bytes.length + 1);
      result[0] = 1; result.set(op.result.bytes, 1);
      return result;
    },
    processOperationCancel: id => processOperations.get(id)?.controller.abort(processFailure("ECANCELLED", "process operation cancelled")),
    processOperationClaim: id => { const op = processOperations.get(id); if (op) op.claimed = true; },
    processOperationRelease: id => {
      const op = processOperations.get(id); processOperations.delete(id);
      if (!op) return;
      op.released = true; op.controller.abort();
      if (op.result?.child && !op.claimed) processStop(op.result.child).catch(() => {});
      for (const resolve of op.waiters.splice(0)) resolve(0);
    },
    processClose: id => { const state = processChildren.get(id); if (state) processStop(state).catch(() => {}); },
  };
}
