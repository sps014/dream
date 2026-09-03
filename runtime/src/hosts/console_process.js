import { isNode, getNodeFs, getNodeChildProcess } from "../platform.js";

/** Blocks and returns one line from stdin (without the trailing newline), or "" if unavailable. */
function consoleReadLineSync() {
  if (isNode) {
    let line = "";
    const buf = Buffer.alloc(1);
    while (true) {
      let n;
      try { n = getNodeFs().readSync(0, buf, 0, 1, null); } catch (_) { break; } // EOF/EAGAIN
      if (n === 0) break;
      const ch = buf.toString("utf8", 0, 1);
      if (ch === "\n") break;
      if (ch !== "\r") line += ch;
    }
    return line;
  }
  if (typeof prompt === "function") return prompt("") || "";
  return "";
}

/**
 * Blocks and returns one character code from stdin, or 0 for EOF/no character. Node has no
 * synchronous raw (unbuffered) terminal mode, so this reads one byte from fd 0 - interactive
 * terminals still wait for Enter, same as `readLine`, but piped input reads a single byte as-is.
 */
function consoleReadKeySync() {
  if (isNode) {
    const buf = Buffer.alloc(1);
    try {
      const n = getNodeFs().readSync(0, buf, 0, 1, null);
      return n === 0 ? 0 : buf[0];
    } catch (_) {
      return 0;
    }
  }
  if (typeof prompt === "function") {
    const s = prompt("") || "";
    return s.length > 0 ? s.charCodeAt(0) : 0;
  }
  return 0;
}
// ----- system.process: Process.run / Process.spawn -----
//
// Wire formats mirror the native host (`src/execution/host/process.rs`) exactly, since both are
// read by the same `ProcessWireReader` / `ChildProcess` Dream code:
//   * `processRun`: "<exit_code>\n<stdout_len>\n" + stdout bytes + stderr bytes. `exit_code == -1`
//     means the process could not be spawned (the tail is the error message); `-2` means it
//     exited via a signal with no exit code.
//   * `processSpawn`: "<handle>\n" (success, empty tail) or "-1\n<message>" (spawn failure).
//   * `processReadStream(Line)`: read side of a spawned child's stdout/stderr (`stream`: 0/1).
//   * `processWait`: decimal exit code (or `-1`/`-2` sentinels, see above).

let nextProcessHandle = 1;
const childProcesses = new Map();

function splitProcessArgs(joined) {
  return joined ? joined.split("\n") : [];
}

function processWire(header, tail) {
  return Buffer.concat([Buffer.from(`${header}\n`, "utf8"), Buffer.from(tail)]);
}

function processUnsupported(message) {
  return processWire(-1, Buffer.from(message, "utf8"));
}

function processRunSync(cmd, argsJoined, cwd) {
  const cp = getNodeChildProcess();
  if (!isNode || !cp) return processUnsupported("Process.run is not supported in the browser");
  try {
    const result = cp.spawnSync(cmd, splitProcessArgs(argsJoined), {
      cwd: cwd || undefined,
      encoding: "buffer",
    });
    if (result.error) return processUnsupported(result.error.message || String(result.error));
    const exitCode = result.status === null ? -2 : result.status;
    const stdout = result.stdout || Buffer.alloc(0);
    const stderr = result.stderr || Buffer.alloc(0);
    const tail = Buffer.concat([Buffer.from(`${stdout.length}\n`, "utf8"), stdout, stderr]);
    return processWire(exitCode, tail);
  } catch (e) {
    return processUnsupported((e && e.message) || String(e));
  }
}

/** Notifies every pending reader of `stream` (0 = stdout, 1 = stderr) that new data/EOF arrived. */
function notifyStream(state, stream) {
  const waiters = state.activityWaiters;
  state.activityWaiters = waiters.filter((w) => {
    if (w.stream !== stream) return true;
    w.resolve();
    return false;
  });
}

function waitForStreamActivity(state, stream) {
  return new Promise((resolve) => state.activityWaiters.push({ stream, resolve }));
}

function bufKeyFor(stream) { return stream === 1 ? "stderrBuf" : "stdoutBuf"; }
function eofKeyFor(stream) { return stream === 1 ? "stderrEof" : "stdoutEof"; }

function processSpawnSync(cmd, argsJoined, cwd) {
  const cp = getNodeChildProcess();
  if (!isNode || !cp) return processUnsupported("Process.spawn is not supported in the browser");
  try {
    const child = cp.spawn(cmd, splitProcessArgs(argsJoined), { cwd: cwd || undefined });
    const state = {
      child,
      stdoutBuf: Buffer.alloc(0),
      stdoutEof: false,
      stderrBuf: Buffer.alloc(0),
      stderrEof: false,
      activityWaiters: [],
      exitCode: null,
      exitWaiters: [],
    };
    child.stdout.on("data", (chunk) => {
      state.stdoutBuf = Buffer.concat([state.stdoutBuf, chunk]);
      notifyStream(state, 0);
    });
    child.stdout.on("end", () => { state.stdoutEof = true; notifyStream(state, 0); });
    child.stderr.on("data", (chunk) => {
      state.stderrBuf = Buffer.concat([state.stderrBuf, chunk]);
      notifyStream(state, 1);
    });
    child.stderr.on("end", () => { state.stderrEof = true; notifyStream(state, 1); });
    child.on("error", () => {
      state.stdoutEof = true;
      state.stderrEof = true;
      notifyStream(state, 0);
      notifyStream(state, 1);
    });
    child.on("close", (code) => {
      state.exitCode = code === null ? -2 : code;
      const waiters = state.exitWaiters;
      state.exitWaiters = [];
      waiters.forEach((resolve) => resolve());
    });
    const id = nextProcessHandle++;
    childProcesses.set(id, state);
    return processWire(id, Buffer.alloc(0));
  } catch (e) {
    return processUnsupported((e && e.message) || String(e));
  }
}

function processWriteStdinSync(handle, data) {
  const state = childProcesses.get(handle);
  if (!state || !state.child.stdin || state.child.stdin.destroyed) return false;
  try {
    state.child.stdin.write(Buffer.from(data));
    return true;
  } catch (_) {
    return false;
  }
}

async function processReadStreamAsync(handle, stream, maxBytes) {
  const state = childProcesses.get(handle);
  if (!state) return Buffer.alloc(0);
  const bufKey = bufKeyFor(stream);
  const eofKey = eofKeyFor(stream);
  while (state[bufKey].length === 0 && !state[eofKey]) {
    await waitForStreamActivity(state, stream);
  }
  const take = Math.max(0, Math.min(maxBytes | 0, state[bufKey].length));
  const chunk = state[bufKey].subarray(0, take);
  state[bufKey] = state[bufKey].subarray(take);
  return Buffer.from(chunk);
}

async function processReadStreamLineAsync(handle, stream) {
  const state = childProcesses.get(handle);
  if (!state) return Buffer.from("0");
  const bufKey = bufKeyFor(stream);
  const eofKey = eofKeyFor(stream);
  while (true) {
    const buf = state[bufKey];
    const nl = buf.indexOf(0x0a);
    if (nl !== -1) {
      let line = buf.subarray(0, nl);
      state[bufKey] = buf.subarray(nl + 1);
      if (line.length > 0 && line[line.length - 1] === 0x0d) line = line.subarray(0, line.length - 1);
      return Buffer.concat([Buffer.from("1"), line]);
    }
    if (state[eofKey]) {
      if (buf.length === 0) return Buffer.from("0");
      state[bufKey] = Buffer.alloc(0);
      return Buffer.concat([Buffer.from("1"), buf]);
    }
    await waitForStreamActivity(state, stream);
  }
}

function processWaitAsync(handle) {
  const state = childProcesses.get(handle);
  if (!state) return Buffer.from("-1");
  if (state.exitCode !== null) return Buffer.from(String(state.exitCode));
  return new Promise((resolve) => {
    state.exitWaiters.push(() => resolve(Buffer.from(String(state.exitCode))));
  });
}

function processKillSync(handle) {
  const state = childProcesses.get(handle);
  if (!state) return false;
  try {
    return state.child.kill();
  } catch (_) {
    return false;
  }
}

export function makeConsoleProcessHost() {
  return {
    consoleReadLine: () => consoleReadLineSync(),
    consoleReadKey: () => consoleReadKeySync(),
    consoleWriteStderr: (text) => {
      if (isNode && process.stderr) process.stderr.write(String(text));
      else console.error(text);
    },
    consoleExit: (code) => {
      if (isNode) process.exit(code);
      throw new Error(`System.exit(${code}): no process to exit in the browser`);
    },
    processPlatform: () => {
      if (isNode) return 1;
      if (typeof window !== "undefined" || typeof self !== "undefined") return 2;
      return 3;
    },
    processOsFamily: () => {
      if (!isNode) return 2;
      return process.platform === "win32" ? 1 : 0;
    },
    processArgs: () => (isNode ? process.argv.slice(2).join("\n") : ""),
    processExePath: () => (isNode ? (process.execPath || "") : ""),
    processEnvGet: (name) => {
      if (!isNode) return "";
      const v = process.env[name];
      return v === undefined ? "" : ("1" + v);
    },
    processEnvSet: (name, value) => {
      if (isNode) process.env[name] = value;
    },
    processEnvUnset: (name) => {
      if (isNode) delete process.env[name];
    },
    processEnvKeys: () => {
      if (!isNode) return "";
      return Object.keys(process.env).sort().join("\n");
    },
    processTempDir: () => {
      if (!isNode) return "/tmp";
      return process.env.TMPDIR || process.env.TEMP || process.env.TMP || "/tmp";
    },
    processHomeDir: () => {
      if (!isNode) return "";
      return process.env.HOME || process.env.USERPROFILE || "";
    },
    processCwd: () => (isNode ? process.cwd() : "/"),
    processSetCwd: (path) => {
      if (!isNode) return false;
      try { process.chdir(path); return true; } catch (_) { return false; }
    },
    processCpuTimeNanos: () => {
      if (!isNode) return 0n;
      const c = process.cpuUsage();
      return BigInt(c.user + c.system) * 1000n;
    },
    processMemoryBytes: () => {
      if (!isNode) return 0n;
      return BigInt(process.memoryUsage().rss);
    },
    processRun: (cmd, argsJoined, cwd) => processRunSync(cmd, argsJoined, cwd),
    processSpawn: (cmd, argsJoined, cwd) => processSpawnSync(cmd, argsJoined, cwd),
    processWriteStdin: (handle, data) => processWriteStdinSync(handle, data),
    processReadStream: (handle, stream, maxBytes) => processReadStreamAsync(handle, stream, maxBytes),
    processReadStreamLine: (handle, stream) => processReadStreamLineAsync(handle, stream),
    processWait: (handle) => processWaitAsync(handle),
    processKill: (handle) => processKillSync(handle),
  };
}
