import { getNodeFs } from "../platform.js";

const operations = new Map();
let nextOperation = 1;
let pendingOperations = 0;

export function makeFileOperationHost(files) {
  function releaseResult(op) {
    if (op.kind === 0 && op.value >= 0 && !op.claimed) files.close(Number(op.value));
  }
  return {
    fileOperationStart: (kind, id, path, mode, count, data, position) => {
      if (kind < 0 || kind > 3 || count < 0 || count === 2147483647 || operations.size >= 132 || pendingOperations >= 132) return 0;
      const number = nextOperation++;
      const op = { kind, done: false, value: -1n, bytes: [1], claimed: false, released: false, waiters: [] };
      operations.set(number, op);
      pendingOperations++;
      const finish = (value, bytes = [1]) => {
        pendingOperations--;
        op.value = BigInt(value);
        op.bytes = bytes;
        op.done = true;
        if (op.released) releaseResult(op);
        for (const resolve of op.waiters) resolve(0);
        op.waiters = [];
      };
      const fs = getNodeFs();
      if (!fs) {
        try {
          if (kind === 0) finish(files.open(path, mode));
          else if (kind === 1) { const bytes = files.read(id, count); finish(bytes[0] ? -1 : bytes.length - 1, bytes); }
          else if (kind === 2) finish(files.write(id, data));
          else finish(files.seek(id, position));
        } catch (_) { finish(-1); }
        return number;
      }
      if (kind === 0) {
        if (!["r", "w", "a", "r+", "w+", "a+"].includes(mode)) { finish(-3); return number; }
        try {
          fs.open(path, mode, (error, fd) => {
            if (error) finish(error.code === "ENOENT" ? -1 : ["EACCES", "EPERM"].includes(error.code) ? -2 : -4);
            else finish(files.register(fd, path, mode));
          });
        } catch (_) { finish(-4); }
        return number;
      }
      const h = files.handles.get(id);
      if (!h || kind === 1 && !h.mode.includes("r") && !h.mode.includes("+")) { finish(-1); return number; }
      if (kind === 3) { finish(files.seek(id, position)); return number; }
      if (kind === 2 && h.mode === "r") { finish(-1); return number; }
      h.pending = (h.pending || 0) + 1;
      const complete = (value, bytes) => {
        h.pending--;
        if (h.closing && !h.pending) { try { fs.closeSync(h.fd); } catch (_) { /* Abandoned handle cleanup cannot report errors. */ } }
        finish(value, bytes);
      };
      try {
        if (kind === 1) {
          const buffer = Buffer.alloc(count);
          fs.read(h.fd, buffer, 0, count, h.pos, (error, read) => {
            if (error) { complete(-1); return; }
            h.pos += read;
            complete(read, [0, ...buffer.subarray(0, read)]);
          });
        } else {
          const bytes = Uint8Array.from(data);
          const append = h.mode.startsWith("a");
          fs.write(h.fd, bytes, 0, bytes.length, append ? null : h.pos, (error, written) => {
            if (error) { complete(-1); return; }
            if (!append) { h.pos += written; complete(written); return; }
            fs.fstat(h.fd, (statError, stat) => {
              if (!statError) h.pos = Number(stat.size);
              // A metadata failure must not erase already completed write progress.
              else h.pos += written;
              complete(written);
            });
          });
        }
      } catch (_) { complete(-1); }
      return number;
    },
    fileOperationDone: (id) => operations.get(id)?.done ? 1 : 0,
    fileOperationResult: (id) => {
      const op = operations.get(id);
      if (!op?.done) return -1n;
      op.claimed = true;
      return op.value;
    },
    fileOperationBytes: (id) => operations.get(id)?.bytes || [1],
    fileOperationRelease: (id) => {
      const op = operations.get(id);
      if (!op) return;
      op.released = true;
      operations.delete(id);
      if (op.done) releaseResult(op);
    },
    fileOperationWait: (id) => {
      const op = operations.get(id);
      if (!op || op.done) return Promise.resolve(0);
      return new Promise((resolve) => op.waiters.push(resolve));
    },
  };
}
