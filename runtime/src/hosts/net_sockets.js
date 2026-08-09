/**
 * Raw TCP (`system.net.TcpClient`) and WebSocket (`system.net.WebSocket`) host functions.
 *
 * TCP is Node-only (`node:net`); the browser has no raw socket API, so `tcpConnect` resolves an
 * error wire there. WebSocket uses whichever global `WebSocket` constructor the host provides
 * (native in browsers; Node >= 22, or a polyfill assigned onto `globalThis.WebSocket`, on Node).
 *
 * Wire formats mirror the native host (`src/execution/host/net.rs`) byte-for-byte so `.dream`
 * parsing code (`NetWireReader`) is shared across both hosts:
 *   connect (`tcpConnect`/`wsConnect`): "<handle>\n" on success, "-1\n<message>" on failure,
 *     "-2\n<message>" for an unsupported host.
 *   `tcpSend`/`tcpSendText`: "<n>\n" bytes written, or "-1\n<message>".
 *   `tcpReceive`: "data\n<bytes>" | "eof\n" | "error\n<message>".
 *   `wsSendText`/`wsSendBinary`: "1\n" on success, "0\n<message>" on failure.
 *   `wsReceive`: "text\n<utf8>" | "binary\n<bytes>" | "close\n<code>\n<reason>" | "error\n<message>".
 */
import { isNode, getNodeNet } from "../platform.js";

const textEncoder = new TextEncoder();

function encodeWire(tagLine, payloadBytes) {
  const tagBytes = textEncoder.encode(tagLine + "\n");
  const payload = payloadBytes || new Uint8Array(0);
  const out = new Uint8Array(tagBytes.length + payload.length);
  out.set(tagBytes, 0);
  out.set(payload, tagBytes.length);
  return out;
}

function encodeWireText(tagLine, text) {
  return encodeWire(tagLine, textEncoder.encode(text || ""));
}

/* ---------------------------------------------------------------------------------------- TCP */

const tcpHandles = new Map();
let nextTcpHandle = 1;

class TcpConn {
  constructor(socket) {
    this.socket = socket;
    this.chunks = [];
    this.eof = false;
    this.error = null;
    this.waiters = [];
    socket.on("data", (chunk) => {
      this.chunks.push(new Uint8Array(chunk));
      this._flush();
    });
    socket.on("end", () => { this.eof = true; this._flush(); });
    socket.on("close", () => { this.eof = true; this._flush(); });
    socket.on("error", (err) => {
      this.error = err;
      this.eof = true;
      this._flush();
    });
  }

  _flush() {
    while (this.waiters.length && (this.chunks.length || this.eof)) {
      this.waiters.shift()();
    }
  }

  async readSome(maxBytes) {
    while (this.chunks.length === 0 && !this.eof) {
      await new Promise((resolve) => this.waiters.push(resolve));
    }
    if (this.chunks.length === 0) {
      if (this.error) {
        return { tag: "error", bytes: textEncoder.encode(String(this.error.message || this.error)) };
      }
      return { tag: "eof", bytes: new Uint8Array(0) };
    }
    const cap = Math.max(1, Number(maxBytes) || 65536);
    let total = 0;
    const parts = [];
    while (this.chunks.length && total < cap) {
      const chunk = this.chunks[0];
      const need = cap - total;
      if (chunk.length > need) {
        parts.push(chunk.subarray(0, need));
        this.chunks[0] = chunk.subarray(need);
        total += need;
      } else {
        parts.push(chunk);
        total += chunk.length;
        this.chunks.shift();
      }
    }
    const out = new Uint8Array(total);
    let offset = 0;
    for (const part of parts) {
      out.set(part, offset);
      offset += part.length;
    }
    return { tag: "data", bytes: out };
  }
}

function tcpConnect(host, port, timeoutMs) {
  return new Promise((resolve) => {
    const net = isNode ? getNodeNet() : null;
    if (!net) {
      resolve(encodeWireText("-1", "TCP sockets are not available in this host"));
      return;
    }
    let settled = false;
    let timer = null;
    const socket = new net.Socket();
    const ms = Number(timeoutMs) || 0;
    if (ms > 0) {
      timer = setTimeout(() => {
        if (settled) return;
        settled = true;
        socket.destroy();
        resolve(encodeWireText("-1", "timeout"));
      }, ms);
    }
    socket.once("connect", () => {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      const id = nextTcpHandle++;
      tcpHandles.set(id, new TcpConn(socket));
      resolve(encodeWireText(String(id), ""));
    });
    socket.once("error", (err) => {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      resolve(encodeWireText("-1", String(err.message || err)));
    });
    socket.connect(port, host);
  });
}

async function tcpSend(handle, data) {
  const conn = tcpHandles.get(handle);
  if (!conn) return encodeWireText("-1", "connection not found");
  return new Promise((resolve) => {
    conn.socket.write(Buffer.from(data), (err) => {
      if (err) resolve(encodeWireText("-1", String(err.message || err)));
      else resolve(encodeWireText(String(data.length), ""));
    });
  });
}

async function tcpSendText(handle, text) {
  return tcpSend(handle, textEncoder.encode(text));
}

async function tcpReceive(handle, maxBytes) {
  const conn = tcpHandles.get(handle);
  if (!conn) return encodeWireText("error", "connection not found");
  const { tag, bytes } = await conn.readSome(maxBytes);
  return encodeWire(tag, bytes);
}

function tcpClose(handle) {
  const conn = tcpHandles.get(handle);
  if (!conn) return 0;
  try { conn.socket.destroy(); } catch (_) { /* already closed */ }
  tcpHandles.delete(handle);
  return 1;
}

/* --------------------------------------------------------------------------------- WebSocket */

const wsHandles = new Map();
let nextWsHandle = 1;

class WsConn {
  constructor(socket) {
    this.socket = socket;
    this.queue = [];
    this.waiters = [];
    socket.binaryType = "arraybuffer";
    socket.onmessage = (ev) => {
      const data = ev.data;
      if (typeof data === "string") {
        this.queue.push({ tag: "text", bytes: textEncoder.encode(data) });
      } else {
        const buf = data instanceof ArrayBuffer ? new Uint8Array(data) : new Uint8Array(data.buffer || data);
        this.queue.push({ tag: "binary", bytes: buf });
      }
      this._flush();
    };
    socket.onclose = (ev) => {
      this.queue.push({ tag: "close", code: ev.code || 1000, reason: ev.reason || "" });
      this._flush();
    };
    socket.onerror = () => {
      this.queue.push({ tag: "error", message: "websocket error" });
      this._flush();
    };
  }

  _flush() {
    while (this.waiters.length && this.queue.length) {
      this.waiters.shift()();
    }
  }

  async next() {
    while (this.queue.length === 0) {
      await new Promise((resolve) => this.waiters.push(resolve));
    }
    return this.queue.shift();
  }
}

function resolveWebSocketCtor() {
  if (typeof WebSocket !== "undefined") return WebSocket;
  if (typeof globalThis !== "undefined" && globalThis.WebSocket) return globalThis.WebSocket;
  return null;
}

function wsConnect(url, timeoutMs) {
  return new Promise((resolve) => {
    const Ctor = resolveWebSocketCtor();
    if (!Ctor) {
      resolve(encodeWireText("-2", "WebSocket is not available in this host"));
      return;
    }
    let socket;
    try {
      socket = new Ctor(url);
    } catch (e) {
      resolve(encodeWireText("-1", String((e && e.message) || e)));
      return;
    }
    let settled = false;
    let timer = null;
    const ms = Number(timeoutMs) || 0;
    if (ms > 0) {
      timer = setTimeout(() => {
        if (settled) return;
        settled = true;
        try { socket.close(); } catch (_) { /* ignore */ }
        resolve(encodeWireText("-1", "timeout"));
      }, ms);
    }
    socket.onopen = () => {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      const id = nextWsHandle++;
      wsHandles.set(id, new WsConn(socket));
      resolve(encodeWireText(String(id), ""));
    };
    socket.onerror = () => {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      resolve(encodeWireText("-1", "connection failed"));
    };
  });
}

async function wsSendText(handle, text) {
  const conn = wsHandles.get(handle);
  if (!conn) return encodeWireText("0", "connection not found");
  try {
    conn.socket.send(text);
    return encodeWireText("1", "");
  } catch (e) {
    return encodeWireText("0", String((e && e.message) || e));
  }
}

async function wsSendBinary(handle, data) {
  const conn = wsHandles.get(handle);
  if (!conn) return encodeWireText("0", "connection not found");
  try {
    conn.socket.send(new Uint8Array(data));
    return encodeWireText("1", "");
  } catch (e) {
    return encodeWireText("0", String((e && e.message) || e));
  }
}

async function wsReceive(handle) {
  const conn = wsHandles.get(handle);
  if (!conn) return encodeWireText("error", "connection not found");
  const msg = await conn.next();
  if (msg.tag === "text") return encodeWire("text", msg.bytes);
  if (msg.tag === "binary") return encodeWire("binary", msg.bytes);
  if (msg.tag === "close") return encodeWire(`close\n${msg.code}`, textEncoder.encode(msg.reason || ""));
  return encodeWireText("error", msg.message || "websocket error");
}

function wsClose(handle, code, reason) {
  const conn = wsHandles.get(handle);
  if (!conn) return 0;
  try { conn.socket.close(code || 1000, reason || ""); } catch (_) { /* already closed */ }
  wsHandles.delete(handle);
  return 1;
}

export function makeNetSocketsHost() {
  return {
    tcpConnect: (host, port, timeoutMs) => tcpConnect(host, port, timeoutMs),
    tcpSend: (handle, data) => tcpSend(handle, data),
    tcpSendText: (handle, text) => tcpSendText(handle, text),
    tcpReceive: (handle, maxBytes) => tcpReceive(handle, maxBytes),
    tcpClose: (handle) => tcpClose(handle),
    wsConnect: (url, timeoutMs) => wsConnect(url, timeoutMs),
    wsSendText: (handle, text) => wsSendText(handle, text),
    wsSendBinary: (handle, data) => wsSendBinary(handle, data),
    wsReceive: (handle) => wsReceive(handle),
    wsClose: (handle, code, reason) => wsClose(handle, code, reason),
  };
}
