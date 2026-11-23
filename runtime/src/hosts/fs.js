import { makeFileOperationHost } from "./file_operations.js";
import { getNodeFs } from "../platform.js";

const memFiles = new Map();
const memDirs = new Map([["/", 0]]);
let virtualCwd = "/";

function normalizeVirtualPath(path) {
  const raw = path == null ? "" : String(path);
  const absolute = raw.startsWith("/") ? raw : `${virtualCwd}/${raw}`;
  const parts = [];
  for (const part of absolute.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  return `/${parts.join("/")}`;
}

function memParent(path) {
  return path.slice(0, path.lastIndexOf("/")) || "/";
}

function memRequireParent(path) {
  if (!memDirs.has(memParent(path))) throw new Error("parent directory does not exist");
}

function memOpen(path, create) {
  const present = memFiles.get(path);
  if (present) return present;
  if (!create || memDirs.has(path)) throw new Error("path is not an openable file");
  memRequireParent(path);
  const file = { bytes: new Uint8Array(), modified: Date.now() };
  memFiles.set(path, file);
  return file;
}

function memReplace(file, bytes) {
  file.bytes = bytes;
  file.modified = Date.now();
}

export function browserCwd() {
  return virtualCwd;
}

export function browserSetCwd(path) {
  const target = normalizeVirtualPath(path);
  if (!memDirs.has(target)) return false;
  virtualCwd = target;
  return true;
}

function memStatWire(path) {
  path = normalizeVirtualPath(path);
  const file = memFiles.get(path);
  const directory = memDirs.get(path);
  if (!file && directory === undefined) return "";
  const size = file ? file.bytes.length : 0;
  const time = file ? file.modified : directory;
  return `${size}\n${time}\n${time}\n${time}\n0\n${file ? 0 : 1}`;
}

function nodeStatWire(path) {
  try {
    const fs = getNodeFs();
    const link = fs.lstatSync(path);
    let kind = 3;
    if (link.isFile()) kind = 0;
    else if (link.isDirectory()) kind = 1;
    else if (link.isSymbolicLink()) kind = 2;
    let st = link;
    if (link.isSymbolicLink()) {
      try { st = fs.statSync(path); } catch (_) { st = link; }
    }
    const mode = typeof process !== "undefined" && process.platform === "win32" ? 0 : (st.mode & 0xffff);
    const ms = (v) => Math.trunc(Number(v) || 0);
    return `${Number(st.size)}\n${ms(st.mtimeMs)}\n${ms(st.ctimeMs)}\n${ms(st.atimeMs)}\n${mode}\n${kind}`;
  } catch (_) {
    return "";
  }
}
const memFs = {
  readBytes(path) {
    return memOpen(normalizeVirtualPath(path), false).bytes;
  },
  write(path, bytes) {
    memReplace(memOpen(normalizeVirtualPath(path), true), Uint8Array.from(bytes));
  },
  append(path, bytes) {
    const file = memOpen(normalizeVirtualPath(path), true);
    const next = new Uint8Array(file.bytes.length + bytes.length);
    next.set(file.bytes);
    next.set(bytes, file.bytes.length);
    memReplace(file, next);
  },
  exists(path) {
    path = normalizeVirtualPath(path);
    return memFiles.has(path) || memDirs.has(path);
  },
  remove(path) {
    path = normalizeVirtualPath(path);
    if (memFiles.delete(path)) return true;
    if (path === "/" || !memDirs.has(path) || this.list(path).length) return false;
    return memDirs.delete(path);
  },
  size(path) {
    return memFiles.get(normalizeVirtualPath(path))?.bytes.length ?? -1;
  },
  isDir(path) {
    return memDirs.has(normalizeVirtualPath(path));
  },
  list(path) {
    path = normalizeVirtualPath(path);
    if (!memDirs.has(path)) throw new Error("path is not a directory");
    const names = [];
    for (const key of [...memFiles.keys(), ...memDirs.keys()]) {
      if (key !== "/" && memParent(key) === path) names.push(key.slice(key.lastIndexOf("/") + 1));
    }
    return names.sort();
  },
  mkdir(path) {
    path = normalizeVirtualPath(path);
    memRequireParent(path);
    if (this.exists(path)) throw new Error("path already exists");
    memDirs.set(path, Date.now());
  },
  mkdirAll(path) {
    path = normalizeVirtualPath(path);
    let current = "";
    for (const part of path.split("/").filter(Boolean)) {
      current += `/${part}`;
      if (!memDirs.has(current)) this.mkdir(current);
    }
  },
  copy(from, to) {
    this.write(to, this.readBytes(from));
  },
  rename(from, to) {
    from = normalizeVirtualPath(from);
    to = normalizeVirtualPath(to);
    if (!this.exists(from)) throw new Error("source does not exist");
    if (from === to) return;
    memRequireParent(to);
    if (from === "/" || to === "/" || to.startsWith(from + "/")) throw new Error("invalid destination");
    const file = memFiles.get(from);
    if (file) {
      if (memDirs.has(to)) throw new Error("destination is a directory");
      memFiles.set(to, file);
      memFiles.delete(from);
      return;
    }
    if (memFiles.has(to) || (memDirs.has(to) && this.list(to).length)) {
      throw new Error("destination is not an empty directory");
    }
    for (const entries of [memFiles, memDirs]) {
      for (const [path, entry] of [...entries]) {
        if (path !== from && !path.startsWith(from + "/")) continue;
        entries.set(to + path.slice(from.length), entry);
        entries.delete(path);
      }
    }
    if (virtualCwd === from || virtualCwd.startsWith(from + "/")) {
      virtualCwd = to + virtualCwd.slice(from.length);
    }
  },
  rmdir(path) {
    path = normalizeVirtualPath(path);
    if (path === "/" || this.list(path).length) throw new Error("directory is not removable");
    memDirs.delete(path);
  },
  rmAll(path) {
    path = normalizeVirtualPath(path);
    if (path === "/" || !this.exists(path)) throw new Error("path is not removable");
    for (const entries of [memFiles, memDirs]) {
      for (const key of entries.keys()) {
        if (key === path || key.startsWith(path + "/")) entries.delete(key);
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
    remove: (p) => {
      try {
        if (fs.lstatSync(p).isDirectory()) fs.rmdirSync(p);
        else fs.unlinkSync(p);
        return true;
      } catch (_) { return false; }
    },
    size: (p) => { try { return Number(fs.statSync(p).size); } catch (_) { return -1; } },
    isDir: (p) => { try { return fs.statSync(p).isDirectory(); } catch (_) { return false; } },
    list: (p) => fs.readdirSync(p).sort(),
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
      fileHandles.set(id, { kind: "node", fd, path, mode, pos: 0 });
      return id;
    } catch (e) {
      return mapOpenError(e);
    }
  }
  const normalized = normalizeVirtualPath(path);
  if (memDirs.has(normalized)) return OPEN_EIO;
  try {
    const file = memOpen(normalized, modeCreates(mode));
    if (mode.startsWith("w")) memReplace(file, new Uint8Array());
    const id = nextFileHandleId++;
    fileHandles.set(id, { kind: "mem", file, mode, pos: 0 });
    return id;
  } catch (_) {
    return OPEN_ENOENT;
  }
}

function fileHandleReadHost(id, n) {
  const h = fileHandles.get(id);
  if (!h || n < 0 || n === 2147483647 || !h.mode.includes("r") && !h.mode.includes("+")) return [1];
  try {
    if (h.kind === "node") {
      const buf = Buffer.alloc(n);
      const count = getNodeFs().readSync(h.fd, buf, 0, n, h.pos);
      h.pos += count;
      return [0, ...buf.subarray(0, count)];
    }
    const bytes = h.file.bytes;
    const end = Math.min(h.pos + n, bytes.length);
    const slice = bytes.subarray(h.pos, end);
    h.pos += slice.length;
    return [0, ...slice];
  } catch (_) {
    return [1];
  }
}

function fileHandleWriteHost(id, data) {
  const h = fileHandles.get(id);
  if (!h || h.mode === "r") return BigInt(-1);
  const bytes = Uint8Array.from(data || []);
  if (bytes.length === 0) return 0n;
  if (h.kind === "node") {
    try {
      const append = h.mode.startsWith("a");
      const written = getNodeFs().writeSync(h.fd, bytes, 0, bytes.length, append ? null : h.pos);
      h.pos = append ? Number(getNodeFs().fstatSync(h.fd).size) : h.pos + written;
      return BigInt(written);
    } catch (_) {
      return BigInt(-1);
    }
  }
  const prev = h.file.bytes;
  if (h.mode.startsWith("a")) h.pos = prev.length;
  const before = prev.subarray(0, Math.min(h.pos, prev.length));
  const afterStart = h.pos + bytes.length;
  const after = prev.length > afterStart ? prev.subarray(afterStart) : new Uint8Array(0);
  const next = new Uint8Array(Math.max(prev.length, afterStart));
  next.set(before, 0);
  next.set(bytes, h.pos);
  next.set(after, afterStart);
  memReplace(h.file, next);
  h.pos = afterStart;
  return BigInt(bytes.length);
}

function fileHandleSeekHost(id, pos) {
  const h = fileHandles.get(id);
  if (!h) return -1;
  const target = Number(pos);
  if (!Number.isSafeInteger(target) || target < 0) return -1;
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
    size = h.file.bytes.length;
  }
  const target = size + Number(offset);
  if (!Number.isSafeInteger(target) || target < 0) return -1;
  h.pos = target;
  return 0;
}

function fileHandleLengthHost(id) {
  const h = fileHandles.get(id);
  if (!h) return -1n;
  try {
    return h.kind === "node" ? getNodeFs().fstatSync(h.fd, { bigint: true }).size
      : BigInt(h.file.bytes.length);
  } catch (_) { return -1n; }
}

function fileHandleCloseHost(id) {
  const h = fileHandles.get(id);
  if (!h) return -1;
  fileHandles.delete(id);
  try {
    if (h.kind === "node") {
      if (h.pending) h.closing = true;
      else getNodeFs().closeSync(h.fd);
    }
    return 0;
  } catch (_) { return -1; }
}

function fsSuccess(bytes) {
  const result = new Uint8Array(bytes.length + 1);
  result.set(bytes, 1);
  return result;
}

export function makeFsHost() {
  return {
    fileAppend: (path, data) => {
      try {
        fsBackend().append(path, Uint8Array.from(data));
        return BigInt(data.length);
      } catch (_) { return -1n; }
    },
    fileReadBytes: (path) => {
      try {
        const backend = fsBackend();
        if (backend.isDir(path)) return [1];
        return fsSuccess(backend.readBytes(path));
      } catch (_) { return [1]; }
    },
    fileWriteBytes: (path, data) => {
      try {
        fsBackend().write(path, Uint8Array.from(data));
        return BigInt(data.length);
      } catch (_) { return -1n; }
    },
    fileExists: (path) => fsBackend().exists(path),
    fileDelete: (path) => fsBackend().remove(path),
    fileSize: (path) => BigInt(fsBackend().size(path)),
    fileIsDir: (path) => fsBackend().isDir(path),
    fileStat: (path) => getNodeFs() ? nodeStatWire(path) : memStatWire(path),
    fileCopy: (from, to) => {
      try { fsBackend().copy(from, to); return true; } catch (_) { return false; }
    },
    tempSupported: () => !!getNodeFs(),
    tempDirectoryCreate: directory => {
      const fs = getNodeFs();
      if (!fs) return "";
      try {
        const base = directory || process.env.TMPDIR || process.env.TEMP || process.env.TMP || "/tmp";
        return fs.mkdtempSync(base.replace(/[\\/]$/, "") + "/.dream-");
      } catch (_) { return ""; }
    },
    tempReplace: (from, to) => {
      const fs = getNodeFs();
      if (!fs) return false;
      try { fs.renameSync(from, to); return true; } catch (_) { return false; }
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
    dirList: (path) => {
      try {
        return fsSuccess(new TextEncoder().encode(fsBackend().list(path).join("\0")));
      } catch (_) { return [1]; }
    },
    dirCreate: (path) => {
      try { fsBackend().mkdir(path); return true; } catch (_) { return false; }
    },
    dirCreateAll: (path) => {
      try { fsBackend().mkdirAll(path); return true; } catch (_) { return false; }
    },
    ...makeFileOperationHost({
      handles: fileHandles,
      open: fileOpenHost, read: fileHandleReadHost, write: fileHandleWriteHost,
      seek: fileHandleSeekHost, close: fileHandleCloseHost,
      register: (fd, path, mode) => {
        const id = nextFileHandleId++;
        fileHandles.set(id, { kind: "node", fd, path, mode, pos: 0 });
        return id;
      },
    }),
    fileOpen: (path, mode) => fileOpenHost(path, mode),
    fileHandleRead: (id, n) => fileHandleReadHost(id, n),
    fileHandleWrite: (id, data) => fileHandleWriteHost(id, data),
    fileHandleSeek: (id, pos) => fileHandleSeekHost(id, pos),
    fileHandleTell: (id) => fileHandleTellHost(id),
    fileHandleSeekEnd: (id, offset) => fileHandleSeekEndHost(id, offset),
    fileHandleLength: (id) => fileHandleLengthHost(id),
    fileHandleClose: (id) => fileHandleCloseHost(id),
  };
}
