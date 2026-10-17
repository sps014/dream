import { isNode, getNodeFs } from "../platform.js";
import { encodeProcessArgs } from "./process_wire.js";
import { makeProcessHost } from "./process_service.js";

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
export function makeConsoleProcessHost() {
  return {
    consoleReadLine: () => consoleReadLineSync(),
    consoleReadKey: () => consoleReadKeySync(),
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
    processArgs: () => encodeProcessArgs(isNode ? process.argv.slice(2) : []),
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
    ...makeProcessHost(),
  };
}
