import { getNodeFs } from "../platform.js";

/**
 * In-memory virtual filesystem used when there is no real FS host (i.e. in the browser), mirroring
 * how a C/C++ -> WASM toolchain (Emscripten's MEMFS) gives you a working filesystem inside the page.
 * Files persist for the page session only. Paths are keys; directories are inferred from prefixes.
 */
const memFiles = new Map(); // path -> Uint8Array
const memDirs = new Set(); // explicit directory markers
const memTimes = new Map(); // path -> epoch millis

function memTouch(path) {
  memTimes.set(path, Date.now());
}

function memStatWire(path) {
  if (!memFs.exists(path)) return "";
  const isDir = memFs.isDir(path);
  const size = isDir ? 0 : memFs.size(path);
  const t = Math.trunc(memTimes.get(path) || 0);
  const kind = isDir ? 1 : 0;
  return `${size}\n${t}\n${t}\n${t}\n0\n${kind}`;
}

function nodeStatWire(path) {
  try {
    const st = getNodeFs().statSync(path);
    let kind = 3;
    if (st.isFile()) kind = 0;
    else if (st.isDirectory()) kind = 1;
    else if (st.isSymbolicLink()) kind = 2;
    const mode = typeof process !== "undefined" && process.platform === "win32" ? 0 : (st.mode & 0xffff);
    const ms = (v) => Math.trunc(Number(v) || 0);
    return `${Number(st.size)}\n${ms(st.mtimeMs)}\n${ms(st.ctimeMs)}\n${ms(st.atimeMs)}\n${mode}\n${kind}`;
  } catch (_) {
    return "";
  }
}
const memFs = {
  readBytes(path) {
    const bytes = memFiles.get(path);
    if (!bytes) throw new Error(`ENOENT: no such file '${path}'`);
    return bytes;
  },
  write(path, bytes) {
    memFiles.set(path, Uint8Array.from(bytes));
    memTouch(path);
  },
  append(path, bytes) {
    const prev = memFiles.get(path) || new Uint8Array(0);
    const next = new Uint8Array(prev.length + bytes.length);
    next.set(prev, 0);
    next.set(bytes, prev.length);
    memFiles.set(path, next);
    memTouch(path);
  },
  exists(path) {
    return memFiles.has(path) || memDirs.has(path) || this.isDir(path);
  },
  remove(path) {
    memDirs.delete(path);
    memTimes.delete(path);
    return memFiles.delete(path);
  },
  size(path) {
    const bytes = memFiles.get(path);
    return bytes ? bytes.length : -1;
  },
  isDir(path) {
    if (memDirs.has(path)) return true;
    const prefix = path.endsWith("/") ? path : path + "/";
    for (const key of memFiles.keys()) {
      if (key.startsWith(prefix)) return true;
    }
    return false;
  },
  list(path) {
    const prefix = path === "" || path === "." ? "" : path.endsWith("/") ? path : path + "/";
    const names = new Set();
    for (const key of memFiles.keys()) {
      if (!key.startsWith(prefix)) continue;
      const rest = key.slice(prefix.length);
      const slash = rest.indexOf("/");
      names.add(slash === -1 ? rest : rest.slice(0, slash));
    }
    for (const dir of memDirs) {
      if (!dir.startsWith(prefix)) continue;
      const rest = dir.slice(prefix.length);
      if (!rest) continue;
      const slash = rest.indexOf("/");
      names.add(slash === -1 ? rest : rest.slice(0, slash));
    }
    return Array.from(names).sort();
  },
  mkdir(path) {
    if (memFiles.has(path) || memDirs.has(path)) throw new Error(`EEXIST: '${path}'`);
    memDirs.add(path);
    memTouch(path);
  },
  mkdirAll(path) {
    const parts = path.split("/").filter(Boolean);
    let cur = path.startsWith("/") ? "" : "";
    for (const part of parts) {
      cur = cur === "" ? (path.startsWith("/") ? "/" + part : part) : cur + "/" + part;
      if (!memDirs.has(cur) && !memFiles.has(cur)) {
        memDirs.add(cur);
        memTouch(cur);
      }
    }
  },
  copy(from, to) {
    if (this.isDir(from)) throw new Error(`EISDIR: '${from}'`);
    this.write(to, this.readBytes(from));
  },
  rename(from, to) {
    if (memFiles.has(from)) {
      memFiles.set(to, memFiles.get(from));
      memFiles.delete(from);
      if (memTimes.has(from)) {
        memTimes.set(to, memTimes.get(from));
        memTimes.delete(from);
      }
      return;
    }
    if (!this.isDir(from)) throw new Error(`ENOENT: '${from}'`);
    const prefix = from.endsWith("/") ? from : from + "/";
    const movedFiles = [];
    for (const key of memFiles.keys()) {
      if (key === from || key.startsWith(prefix)) movedFiles.push(key);
    }
    for (const key of movedFiles) {
      const next = key === from ? to : to + key.slice(from.length);
      memFiles.set(next, memFiles.get(key));
      memFiles.delete(key);
      if (memTimes.has(key)) {
        memTimes.set(next, memTimes.get(key));
        memTimes.delete(key);
      }
    }
    const movedDirs = [];
    for (const dir of memDirs) {
      if (dir === from || dir.startsWith(prefix)) movedDirs.push(dir);
    }
    for (const dir of movedDirs) {
      const next = dir === from ? to : to + dir.slice(from.length);
      memDirs.delete(dir);
      memDirs.add(next);
      if (memTimes.has(dir)) {
        memTimes.set(next, memTimes.get(dir));
        memTimes.delete(dir);
      }
    }
  },
  rmdir(path) {
    if (!this.isDir(path)) throw new Error(`ENOTDIR: '${path}'`);
    const names = this.list(path);
    if (names.length > 0) throw new Error(`ENOTEMPTY: '${path}'`);
    memDirs.delete(path);
    memTimes.delete(path);
  },
  rmAll(path) {
    if (memFiles.has(path)) {
      this.remove(path);
      return;
    }
    if (!this.exists(path)) throw new Error(`ENOENT: '${path}'`);
    const prefix = path.endsWith("/") ? path : path + "/";
    for (const key of Array.from(memFiles.keys())) {
      if (key === path || key.startsWith(prefix)) {
        memFiles.delete(key);
        memTimes.delete(key);
      }
    }
    for (const dir of Array.from(memDirs)) {
      if (dir === path || dir.startsWith(prefix)) {
        memDirs.delete(dir);
        memTimes.delete(dir);
      }
    }
  },
};

// Real-filesystem backend over Node's `fs`, normalized to the same byte-oriented shape as `memFs`.
let _nodeFsBackend = null;
function nodeFsBackend() {
  if (_nodeFsBackend) return _nodeFsBackend;
  const fs = getNodeFs();
  _nodeFsBackend = {
    readBytes: (p) => new Uint8Array(fs.readFileSync(p)),
    write: (p, bytes) => fs.writeFileSync(p, Buffer.from(bytes)),
    append: (p, bytes) => fs.appendFileSync(p, Buffer.from(bytes)),
    exists: (p) => fs.existsSync(p),
    remove: (p) => { try { fs.rmSync(p); return true; } catch (_) { return false; } },
    size: (p) => { try { return Number(fs.statSync(p).size); } catch (_) { return -1; } },
    isDir: (p) => { try { return fs.statSync(p).isDirectory(); } catch (_) { return false; } },
    list: (p) => { try { return fs.readdirSync(p).sort(); } catch (_) { return []; } },
    mkdir: (p) => { fs.mkdirSync(p); },
    mkdirAll: (p) => { fs.mkdirSync(p, { recursive: true }); },
    copy: (from, to) => { fs.copyFileSync(from, to); },
    rename: (from, to) => { fs.renameSync(from, to); },
    rmdir: (p) => { fs.rmdirSync(p); },
    rmAll: (p) => { fs.rmSync(p, { recursive: true, force: false }); },
  };
  return _nodeFsBackend;
}

/** The active filesystem backend: Node's real `fs` when available, else the in-memory `memFs`. */
function fsBackend() {
  return getNodeFs() ? nodeFsBackend() : memFs;
}

// --- streaming file handles (system/io/file_handle.dream) ------------------------------------

const OPEN_ENOENT = -1;
const OPEN_EACCES = -2;
const OPEN_EINVAL = -3;
const OPEN_EIO = -4;

/** Live open handles keyed by id (Node fd or in-memory VFS cursor). */
const fileHandles = new Map();
let nextFileHandleId = 1;

function mapNodeOpenMode(mode) {
  switch (mode) {
    case "r": return "r";
    case "w": return "w";
    case "a": return "a";
    case "r+": return "r+";
    case "w+": return "w+";
    case "a+": return "a+";
    default: return null;
  }
}

function mapOpenError(e) {
  if (!e || !e.code) return OPEN_EIO;
  if (e.code === "ENOENT") return OPEN_ENOENT;
  if (e.code === "EACCES" || e.code === "EPERM") return OPEN_EACCES;
  return OPEN_EIO;
}

function modeCreates(mode) {
  return mode === "w" || mode === "a" || mode === "w+" || mode === "a+";
}

function fileOpenHost(path, mode) {
  const nodeMode = mapNodeOpenMode(mode);
  if (!nodeMode) return OPEN_EINVAL;
  if (getNodeFs()) {
    try {
      const fd = getNodeFs().openSync(path, nodeMode);
      const id = nextFileHandleId++;
      fileHandles.set(id, { kind: "node", fd, path, pos: 0 });
      return id;
    } catch (e) {
      return mapOpenError(e);
    }
  }
  // Browser VFS: open a cursor without copying file bytes into Dream memory up front.
  if (!memFiles.has(path) && !modeCreates(mode)) return OPEN_ENOENT;
  if (!memFiles.has(path)) memFiles.set(path, new Uint8Array(0));
  const id = nextFileHandleId++;
  fileHandles.set(id, { kind: "mem", path, pos: 0 });
  return id;
}

function fileHandleReadHost(id, n) {
  const h = fileHandles.get(id);
  if (!h) return [];
  const count = n > 0 ? n : 0;
  if (h.kind === "node") {
    const buf = Buffer.alloc(count);
    const bytesRead = getNodeFs().readSync(h.fd, buf, 0, count, h.pos);
    h.pos += bytesRead;
    return Array.from(buf.subarray(0, bytesRead));
  }
  const bytes = memFiles.get(h.path);
  if (!bytes) return [];
  const end = Math.min(h.pos + count, bytes.length);
  const slice = bytes.subarray(h.pos, end);
  h.pos = end;
  return Array.from(slice);
}

function fileHandleWriteHost(id, data) {
  const h = fileHandles.get(id);
  if (!h) return BigInt(-1);
  const bytes = Uint8Array.from(data || []);
  if (h.kind === "node") {
    try {
      const written = getNodeFs().writeSync(h.fd, bytes, 0, bytes.length, h.pos);
      h.pos += written;
      return BigInt(written);
    } catch (_) {
      return BigInt(-1);
    }
  }
  const prev = memFiles.get(h.path) || new Uint8Array(0);
  const before = prev.subarray(0, Math.min(h.pos, prev.length));
  const afterStart = h.pos + bytes.length;
  const after = prev.length > afterStart ? prev.subarray(afterStart) : new Uint8Array(0);
  const next = new Uint8Array(before.length + bytes.length + after.length);
  next.set(before, 0);
  next.set(bytes, before.length);
  next.set(after, before.length + bytes.length);
  memFiles.set(h.path, next);
  h.pos = before.length + bytes.length;
  return BigInt(bytes.length);
}

function fileHandleSeekHost(id, pos) {
  const h = fileHandles.get(id);
  if (!h) return -1;
  const target = Number(pos);
  if (target < 0) return -1;
  h.pos = target;
  return 0;
}

function fileHandleTellHost(id) {
  const h = fileHandles.get(id);
  if (!h) return BigInt(-1);
  return BigInt(h.pos);
}

function fileHandleSeekEndHost(id, offset) {
  const h = fileHandles.get(id);
  if (!h) return -1;
  let size = 0;
  if (h.kind === "node") {
    try { size = Number(getNodeFs().fstatSync(h.fd).size); } catch (_) { return -1; }
  } else {
    const bytes = memFiles.get(h.path);
    size = bytes ? bytes.length : 0;
  }
  const target = size + Number(offset);
  if (target < 0) return -1;
  h.pos = target;
  return 0;
}

function fileHandleCloseHost(id) {
  const h = fileHandles.get(id);
  if (!h) return;
  if (h.kind === "node") {
    try { getNodeFs().closeSync(h.fd); } catch (_) { /* ignore */ }
  }
  fileHandles.delete(id);
}
export function makeFsHost() {
  return {
    fileRead: (path) => new TextDecoder("utf-8").decode(fsBackend().readBytes(path)),
    fileWrite: (path, content) => {
      const bytes = new TextEncoder().encode(content);
      fsBackend().write(path, bytes);
      return BigInt(bytes.length);
    },
    fileAppend: (path, content) => {
      const bytes = new TextEncoder().encode(content);
      fsBackend().append(path, bytes);
      return BigInt(bytes.length);
    },
    fileReadBytes: (path) => fsBackend().readBytes(path),
    fileWriteBytes: (path, data) => {
      fsBackend().write(path, Uint8Array.from(data));
      return BigInt(data.length);
    },
    fileExists: (path) => fsBackend().exists(path),
    fileDelete: (path) => fsBackend().remove(path),
    fileSize: (path) => BigInt(fsBackend().size(path)),
    fileIsDir: (path) => fsBackend().isDir(path),
    fileStat: (path) => getNodeFs() ? nodeStatWire(path) : memStatWire(path),
    fileCopy: (from, to) => {
      try { fsBackend().copy(from, to); return true; } catch (_) { return false; }
    },
    fileRename: (from, to) => {
      try { fsBackend().rename(from, to); return true; } catch (_) { return false; }
    },
    dirRemove: (path) => {
      try { fsBackend().rmdir(path); return true; } catch (_) { return false; }
    },
    dirRemoveAll: (path) => {
      try { fsBackend().rmAll(path); return true; } catch (_) { return false; }
    },
    dirList: (path) => fsBackend().list(path).join("\n"),
    dirCreate: (path) => {
      try { fsBackend().mkdir(path); return true; } catch (_) { return false; }
    },
    dirCreateAll: (path) => {
      try { fsBackend().mkdirAll(path); return true; } catch (_) { return false; }
    },
    fileOpen: (path, mode) => fileOpenHost(path, mode),
    fileHandleRead: (id, n) => fileHandleReadHost(id, n),
    fileHandleWrite: (id, data) => fileHandleWriteHost(id, data),
    fileHandleSeek: (id, pos) => fileHandleSeekHost(id, pos),
    fileHandleTell: (id) => fileHandleTellHost(id),
    fileHandleSeekEnd: (id, offset) => fileHandleSeekEndHost(id, offset),
    fileHandleClose: (id) => { fileHandleCloseHost(id); },
  };
}
