import { getNodeFs } from "../platform.js";

/** Default `env` builtins every Dream module imports (mirrors the WASM guest ABI). */

/** Native host parity: `%.6f` with trailing zeros (and dot) trimmed. */
function formatFloat(v) {
  if (!Number.isFinite(v)) return String(v);
  let text = v.toFixed(6);
  if (text.includes(".")) {
    text = text.replace(/0+$/, "").replace(/\.$/, "");
  }
  return text === "-0" ? "0" : text;
}

/** Native host parity: `%.16g`. */
function formatDouble(v) {
  if (!Number.isFinite(v)) return String(v);
  return String(Number(v.toPrecision(16)));
}

function nodeStdoutWrite(s) {
  const fs = getNodeFs();
  const fd =
    typeof process !== "undefined" && process.stdout && typeof process.stdout.fd === "number"
      ? process.stdout.fd
      : 1;
  if (fs && typeof fs.writeSync === "function") {
    fs.writeSync(fd, s);
    return;
  }
  if (typeof process !== "undefined" && process.stdout) {
    process.stdout.write(s);
    return;
  }
  console.log(s);
}

/** Diagnostic stream for panics and a failing `main` — never mixed into program output. */
function nodeStderrWrite(s) {
  const fs = getNodeFs();
  const fd =
    typeof process !== "undefined" && process.stderr && typeof process.stderr.fd === "number"
      ? process.stderr.fd
      : 2;
  if (fs && typeof fs.writeSync === "function") {
    fs.writeSync(fd, s);
    return;
  }
  if (typeof process !== "undefined" && process.stderr) {
    process.stderr.write(s);
    return;
  }
  console.error(s);
}

function defaultEnv(getInstance, options) {
  const utf8 = new TextDecoder("utf-8");
  const utf16 = new TextDecoder("utf-16le");
  const stdoutBytes = new TextDecoder("utf-8", { ignoreBOM: true });
  const stderrBytes = new TextDecoder("utf-8", { ignoreBOM: true });
  const writeOutRaw = options.stdout || nodeStdoutWrite;
  const writeErrRaw = options.stderr || nodeStderrWrite;
  const writeOut = (text) => {
    const pending = stdoutBytes.decode();
    if (pending) writeOutRaw(pending);
    writeOutRaw(text);
  };
  const writeErr = (text) => {
    const pending = stderrBytes.decode();
    if (pending) writeErrRaw(pending);
    writeErrRaw(text);
  };
  const writeLine = (text) => writeOut(text + "\n");

  return {
    console_read_bytes: (ptr, count) => {
      const fs = getNodeFs();
      if (!fs || count < 0) return -1;
      try {
        return fs.readSync(0, getInstance().bytes, ptr >>> 0, count, null);
      } catch (_) { return -1; }
    },
    console_write_bytes: (stream, ptr, count) => {
      if ((stream !== 1 && stream !== 2) || count < 0) return -1;
      if (count === 0) return 0;
      const bytes = getInstance().bytes.subarray(ptr >>> 0, (ptr >>> 0) + count);
      const callback = stream === 2 ? options.stderr : options.stdout;
      const fs = getNodeFs();
      try {
        if (!callback && fs) return fs.writeSync(stream, bytes, 0, count);
        const decoder = stream === 2 ? stderrBytes : stdoutBytes;
        const text = decoder.decode(bytes, { stream: true });
        if (text) (stream === 2 ? writeErrRaw : writeOutRaw)(text);
        return count;
      } catch (_) { return -1; }
    },
    console_flush_stream: (stream) => {
      if (stream !== 1 && stream !== 2) return -1;
      try {
        const text = (stream === 2 ? stderrBytes : stdoutBytes).decode();
        if (text) (stream === 2 ? writeErrRaw : writeOutRaw)(text);
        return 0;
      } catch (_) { return -1; }
    },
    write_text: (stream, ptr, size, encoding) => {
      const start = ptr >>> 0;
      const length = (size >>> 0) * (encoding === 1 ? 2 : 1);
      const text = (encoding === 1 ? utf16 : utf8).decode(getInstance().bytes.subarray(start, start + length));
      (stream === 2 ? writeErr : writeOut)(text);
    },
    print_string: (ptr) => writeOut(getInstance().readString(ptr)),
    println: (ptr) => writeLine(getInstance().readString(ptr)),
    print_int: (v) => writeOut(String(v)),
    print_float: (v) => writeOut(formatFloat(v)),
    print_double: (v) => writeOut(formatDouble(v)),
    print_char: (v) => writeOut(String.fromCharCode(v)),
    print_err_string: (ptr) => writeErr(getInstance().readString(ptr)),
    print_err_char: (v) => writeErr(String.fromCharCode(v)),
  };
}
