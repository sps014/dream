import { isNode, getNodeChildProcess } from "../platform.js";

// Mirrors runtime/c/sys/native/process_child.c: every launch, pipe read or write, and exit
// wait is a call the guest polls, cancels, and releases by id.
const children = new Map();
const calls = new Map();
let nextChild = 1;
let nextCall = 1;
const SIGNALS = { SIGHUP: 1, SIGINT: 2, SIGQUIT: 3, SIGABRT: 6, SIGKILL: 9, SIGSEGV: 11, SIGPIPE: 13, SIGTERM: 15 };

function failure(code, message) { return { code, message }; }
const cancelled = () => failure("ECANCELLED", "process operation cancelled");
const closed = () => failure("ECLOSED", "child process closed");

function newCall(kind, state) {
  if (calls.size >= 4096 || nextCall >= 2147483647) return null;
  const call = { id: nextCall++, kind, state, done: false, value: -1n, bytes: [], code: "", message: "", waiters: [], claimed: false, released: false, cancel: null, pipe: null };
  calls.set(call.id, call);
  state?.calls.add(call);
  return call;
}

function settle(call, value, bytes, error) {
  if (call.done) return;
  call.done = true;
  call.cancel = null;
  call.state?.calls.delete(call);
  if (error) { call.code = error.code; call.message = error.message; }
  else { call.value = BigInt(value); if (bytes) call.bytes = bytes; }
  if (call.released) dispose(call);
  for (const resolve of call.waiters.splice(0)) resolve(0);
}

// Unclaimed results: read bytes return to the front of their pipe, a launched child is killed.
function dispose(call) {
  if (call.claimed || call.code) return;
  if (call.kind === "read" && call.bytes.length) call.pipe.unshift(Buffer.from(call.bytes));
  if (call.kind === "spawn") release(Number(call.value));
}

function interrupt(state) {
  for (const call of [...state.calls]) if (call.kind !== "wait") call.cancel?.(closed());
}

function kill(state) {
  if (state.status === null) state.proc.kill("SIGKILL");
  interrupt(state);
}

function release(id) {
  const state = children.get(id);
  if (!state) return;
  children.delete(id);
  kill(state);
}

function textValid(text) { return typeof text === "string" && !text.includes("\0"); }

function launch(call, command, args, cwd, env, clearEnv) {
  if (!isNode || !getNodeChildProcess()) return settle(call, -1, null, failure("EUNSUPPORTED", "processes are unavailable in the browser"));
  if (![command, cwd, ...args, ...env].every(textValid) || env.length % 2) return settle(call, -1, null, failure("EINVAL", "process text contains a NUL character"));
  const environment = clearEnv ? {} : { ...process.env };
  for (let i = 0; i < env.length; i += 2) {
    if (!env[i] || env[i].includes("=")) return settle(call, -1, null, failure("EINVAL", `invalid environment override: ${env[i]}`));
    Object.defineProperty(environment, env[i], { value: env[i + 1], enumerable: true, writable: true, configurable: true });
  }
  let proc;
  try { proc = getNodeChildProcess().spawn(command, args, { cwd: cwd || undefined, env: environment, stdio: "pipe", windowsHide: true }); }
  catch (error) { return settle(call, -1, null, failure(error?.code === "EINVAL" ? "EINVAL" : "ESPAWN", error.message || String(error))); }
  const state = { id: nextChild++, proc, status: null, error: null, calls: new Set() };
  state.exited = new Promise(resolve => {
    proc.once("error", error => { state.error ??= failure("ESPAWN", error.message); });
    proc.once("close", (code, signal) => { state.status = code ?? -(SIGNALS[signal] || 2); resolve(state.status); });
  });
  for (const pipe of [proc.stdin, proc.stdout, proc.stderr]) pipe?.on("error", () => {});
  children.set(state.id, state);
  const detach = () => { proc.off("spawn", spawned); proc.off("error", errored); };
  const spawned = () => { detach(); settle(call, state.id); };
  const errored = error => { detach(); children.delete(state.id); settle(call, -1, null, failure("ESPAWN", error.message)); };
  proc.once("spawn", spawned);
  proc.once("error", errored);
  call.cancel = error => { detach(); release(state.id); settle(call, -1, null, error); };
}

function read(call, max) {
  const { state, pipe } = call;
  if (call.done) return;
  if (state.error) return settle(call, -1, null, state.error);
  const available = pipe.readableLength;
  if (available) {
    const chunk = pipe.read(Math.min(max, available));
    return settle(call, chunk.length, Array.from(chunk));
  }
  if (pipe.readableEnded || pipe.destroyed) return settle(call, 0, []);
  const detach = () => { pipe.off("readable", ready); pipe.off("end", ready); pipe.off("close", ready); pipe.off("error", failed); };
  const ready = () => { detach(); read(call, max); };
  const failed = error => { detach(); settle(call, -1, null, failure("EIO", error.message)); };
  pipe.once("readable", ready); pipe.once("end", ready); pipe.once("close", ready); pipe.once("error", failed);
  call.cancel = error => { detach(); settle(call, -1, null, error); };
}

export function makeProcessHost() {
  const start = (kind, id, run) => {
    const state = children.get(id);
    const call = newCall(kind, state);
    if (!call) return 0;
    if (!state) settle(call, -1, null, failure("ECLOSED", "child process is closed"));
    else run(call, state);
    return call.id;
  };
  return {
    processSpawn: (command, args, cwd, env, clearEnv) => {
      const call = newCall("spawn", null);
      if (!call) return 0;
      launch(call, command, args, cwd, env, clearEnv);
      return call.id;
    },
    processRead: (id, stream, max) => start("read", id, (call, state) => {
      if (![0, 1].includes(stream) || max < 0 || max > 16777216) return settle(call, -1, null, failure("EINVAL", "invalid process read"));
      if (max === 0) return settle(call, 0, []);
      call.pipe = stream === 0 ? state.proc.stdout : state.proc.stderr;
      read(call, max);
    }),
    processWrite: (id, data) => start("write", id, (call, state) => {
      const pipe = state.proc.stdin;
      if (pipe.destroyed || pipe.writableEnded) return settle(call, -1, null, failure("ECLOSED", "stdin closed"));
      if (!data.length) return settle(call, 0, []);
      call.cancel = error => settle(call, -1, null, error);
      pipe.write(Buffer.from(data), error => error
        ? settle(call, -1, null, failure(error.code === "EPIPE" ? "EPIPE" : "EIO", error.message))
        : settle(call, data.length));
    }),
    processWait: id => start("wait", id, (call, state) => {
      call.cancel = error => settle(call, -1, null, error);
      state.exited.then(status => state.error ? settle(call, -1, null, state.error) : settle(call, status));
    }),
    processCallWait: id => {
      const call = calls.get(id);
      if (!call || call.done) return Promise.resolve(0);
      return new Promise(resolve => call.waiters.push(resolve));
    },
    processCallValue: id => {
      const call = calls.get(id);
      if (!call?.done) return -1n;
      call.claimed = true;
      return call.code ? -1n : call.value;
    },
    processCallBytes: id => {
      const call = calls.get(id);
      if (!call?.done) return [];
      call.claimed = true;
      return call.code ? [] : call.bytes;
    },
    processCallCode: id => {
      const call = calls.get(id);
      if (!call) return "ELIMIT";
      return call.done ? call.code : "";
    },
    processCallMessage: id => calls.get(id)?.message ?? "process call capacity exhausted",
    processCallCancel: id => { const call = calls.get(id); if (call && !call.done) call.cancel?.(cancelled()); },
    processCallRelease: id => {
      const call = calls.get(id);
      if (!call) return;
      calls.delete(id);
      call.released = true;
      if (!call.done) call.cancel?.(cancelled());
      else dispose(call);
    },
    processCloseStdin: id => { const stdin = children.get(id)?.proc.stdin; if (stdin && !stdin.writableEnded) stdin.end(); },
    processKill: id => { const state = children.get(id); if (state) kill(state); },
    processRelease: release,
  };
}
