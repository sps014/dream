import { getNodeDgram, getNodeNet } from "../platform.js";

export function makeSocketHost() {
  // Mirrors runtime/c/sys/native/socket.c: every open, accept, read, write, datagram send or
  // receive, and option change is a call the guest waits on, cancels, and releases by id.
  const sockets = new Map();
  const calls = new Map();
  let nextSocket = 1;
  let nextCall = 1;
  const READ_CHUNK = 1024 * 1024;
  const OPTION_NODELAY = 0, OPTION_BROADCAST = 1, OPTION_TTL = 2;
  const READ_SIDE = new Set(["accept", "read", "receive"]);

  function failure(code, message) { return { code, message }; }
  const cancelled = () => failure("ECANCELLED", "socket operation cancelled");
  const closed = () => failure("ECLOSED", "socket is closed");
  const io = error => failure("EIO", error?.message || String(error));

  function newCall(kind, state) {
    if (calls.size >= 4096 || nextCall >= 2147483647) return null;
    const call = { id: nextCall++, kind, state, done: false, value: -1n, bytes: [], peer: null, code: "", message: "", waiters: [], claimed: false, released: false, cancel: null, timer: null };
    calls.set(call.id, call);
    state?.calls.add(call);
    return call;
  }

  function settle(call, value, bytes, error, peer) {
    if (call.done) return;
    call.done = true;
    call.cancel = null;
    clearTimeout(call.timer);
    call.state?.calls.delete(call);
    if (error) { call.code = error.code; call.message = error.message; }
    else { call.value = BigInt(value); if (bytes) call.bytes = bytes; if (peer) call.peer = peer; }
    if (call.released) dispose(call);
    for (const resolve of call.waiters.splice(0)) resolve(0);
  }

  // Unclaimed results: read bytes and datagrams go back to their socket, an opened socket closes.
  function dispose(call) {
    if (call.claimed || call.code) return;
    const { state } = call;
    if (call.kind === "read" && call.bytes.length) state.leftover = Buffer.concat([Buffer.from(call.bytes), state.leftover]);
    if (call.kind === "receive" && !state.held) state.held = { bytes: Buffer.from(call.bytes), length: Number(call.value), peer: call.peer };
    if (["connect", "listen", "bind", "accept"].includes(call.kind)) close(Number(call.value));
  }

  function deadline(call, timeout) {
    if (timeout > 0) call.timer = setTimeout(() => call.cancel?.(failure("ETIMEOUT", "socket operation deadline exceeded")), timeout);
  }

  function register(kind, handle) {
    if (sockets.size >= 1048576 || nextSocket >= 2147483647) return 0;
    const local = handle.address();
    const state = {
      id: nextSocket++, kind, handle, calls: new Set(), closed: false, ended: false, error: null, writeShut: false,
      leftover: Buffer.alloc(0), held: null, pending: [],
      local: { host: local?.address ?? "", port: local?.port ?? 0 },
      peer: { host: handle.remoteAddress ?? "", port: handle.remotePort ?? 0 },
    };
    const wake = () => handle.emit("dream-ready");
    handle.on("error", error => { state.error ??= error; wake(); });
    handle.on("close", () => { state.closed = true; wake(); });
    if (kind === "tcp") handle.on("end", () => { state.ended = true; wake(); });
    sockets.set(state.id, state);
    return state.id;
  }

  function close(id) {
    const state = sockets.get(id);
    if (!state) return;
    sockets.delete(id);
    state.closed = true;
    for (const call of [...state.calls]) call.cancel?.(closed());
    for (const pending of state.pending.splice(0)) pending.destroy?.();
    state.queue = [];
    try { state.kind === "tcp" ? state.handle.destroy() : state.handle.close(); } catch (_) { /* already closed */ }
  }

  // Retries `attempt` whenever the handle reports progress until it settles the call.
  function poll(call, events, attempt) {
    const { handle } = call.state;
    const detach = () => { for (const name of events) handle.off(name, ready); };
    const ready = () => { detach(); if (!call.done) attempt(); };
    for (const name of events) handle.once(name, ready);
    call.cancel = error => { detach(); settle(call, -1, null, error); };
  }

  function open(call, kind, host, port, timeout) {
    const net = getNodeNet();
    if (!net) return settle(call, -1, null, failure("EIO", "raw sockets need Node.js"));
    let handle, owner = 0;
    const fail = error => { try { kind === "connect" ? handle.destroy() : handle.close(); } catch (_) { /* never opened */ } settle(call, -1, null, error); };
    const opened = (type) => {
      handle.off("error", errored);
      const id = register(type, handle);
      owner = id;
      if (!id) return fail(failure("ELIMIT", "socket capacity exhausted"));
      if (type === "udp") handle.on("message", (bytes, peer) => {
        const state = sockets.get(id);
        if (state && state.pending.length < 64) { state.pending.push({ bytes: Buffer.from(bytes), peer }); handle.emit("dream-ready"); }
      });
      settle(call, id);
    };
    const errored = error => fail(io(error));
    try {
      if (kind === "bind") {
        handle = getNodeDgram().createSocket(host.includes(":") ? "udp6" : "udp4");
        handle.once("error", errored);
        handle.bind(port, host, () => opened("udp"));
      } else if (kind === "connect") {
        handle = new net.Socket({ allowHalfOpen: true });
        handle.once("error", errored);
        handle.connect(port, host, () => { handle.pause(); opened("tcp"); });
      } else {
        handle = net.createServer({ pauseOnConnect: true, allowHalfOpen: true });
        handle.once("error", errored);
        handle.listen({ host, port }, () => opened("listener"));
        handle.on("connection", socket => {
          const state = sockets.get(owner);
          if (!state || state.closed || state.pending.length >= 64) { socket.destroy(); return; }
          state.pending.push(socket);
          handle.emit("dream-ready");
        });
      }
    } catch (error) { return fail(io(error)); }
    call.cancel = fail;
    deadline(call, timeout);
  }

  function accept(call) {
    const { state } = call;
    while (state.pending.length) {
      const socket = state.pending.shift();
      if (socket.destroyed) continue;
      const id = register("tcp", socket);
      if (!id) { socket.destroy(); return settle(call, -1, null, failure("ELIMIT", "socket capacity exhausted")); }
      socket.on("error", () => {});
      return settle(call, id);
    }
    if (state.closed) return settle(call, -1, null, closed());
    if (state.error) return settle(call, -1, null, io(state.error));
    poll(call, ["dream-ready"], () => accept(call));
  }

  function read(call, max) {
    const { state } = call;
    const { handle } = state;
    if (state.leftover.length) {
      const size = Math.min(max, state.leftover.length);
      const bytes = Array.from(state.leftover.subarray(0, size));
      state.leftover = state.leftover.subarray(size);
      return settle(call, size, bytes);
    }
    if (handle.readableLength) {
      const chunk = handle.read(Math.min(max, handle.readableLength));
      return settle(call, chunk.length, Array.from(chunk));
    }
    if (state.error) return settle(call, -1, null, io(state.error));
    if (state.ended || handle.readableEnded) return settle(call, 0, []);
    if (state.closed) return settle(call, -1, null, closed());
    poll(call, ["readable", "dream-ready"], () => read(call, max));
  }

  function receive(call, max) {
    const { state } = call;
    const take = (bytes, length, peer) => settle(call, length, Array.from(bytes.subarray(0, max)), null, { host: peer.address ?? peer.host, port: peer.port });
    if (state.held) { const { bytes, length, peer } = state.held; state.held = null; return take(bytes, length, peer); }
    if (state.pending.length) { const { bytes, peer } = state.pending.shift(); return take(bytes, bytes.length, peer); }
    if (state.closed) return settle(call, -1, null, closed());
    if (state.error) return settle(call, -1, null, io(state.error));
    poll(call, ["dream-ready"], () => receive(call, max));
  }

  function textValid(text) { return typeof text === "string" && text.length > 0 && !text.includes("\0"); }

  const start = (kind, id, expected, timeout, run) => {
    const state = sockets.get(id);
    const call = newCall(kind, null);
    if (!call) return 0;
    if (!Number.isInteger(timeout) || timeout < 0) settle(call, -1, null, failure("EINVAL", "invalid socket timeout"));
    else if (!state) settle(call, -1, null, closed());
    else if (state.kind !== expected) settle(call, -1, null, failure("EINVAL", "operation does not match socket kind"));
    else if ([...state.calls].some(other => kind === "write" ? other.kind === "write" : READ_SIDE.has(kind) && READ_SIDE.has(other.kind))) {
      settle(call, -1, null, failure("EBUSY", kind === "write" ? "socket already has a pending write" : "socket already has a pending read"));
    } else {
      call.state = state;
      state.calls.add(call);
      run(call, state);
      if (!call.done) deadline(call, timeout);
    }
    return call.id;
  };
  const opening = kind => (host, port, timeout = 0) => {
    const call = newCall(kind, null);
    if (!call) return 0;
    if (!textValid(host) || !Number.isInteger(port) || port < 0 || port > 65535 || timeout < 0) settle(call, -1, null, failure("EINVAL", "invalid socket endpoint"));
    else if (kind === "bind" && !getNodeDgram()) settle(call, -1, null, failure("EIO", "raw sockets need Node.js"));
    else open(call, kind, host, port, timeout);
    return call.id;
  };
  const control = (id, expected, apply) => {
    const state = sockets.get(id);
    const call = newCall("control", null);
    if (!call) return 0;
    try {
      if (!state) settle(call, -1, null, closed());
      else if (state.kind !== expected) settle(call, -1, null, failure("EINVAL", "unsupported socket option"));
      else { const error = apply(state); settle(call, error ? -1 : 0, null, error); }
    } catch (error) { settle(call, -1, null, io(error)); }
    return call.id;
  };
  return {
    netSocketConnect: opening("connect"),
    netSocketListen: opening("listen"),
    netSocketBind: opening("bind"),
    netSocketAccept: (id, timeout) => start("accept", id, "listener", timeout, call => accept(call)),
    netSocketRead: (id, max, timeout) => start("read", id, "tcp", timeout, call => {
      if (max < 0) return settle(call, -1, null, failure("EINVAL", "invalid socket read size"));
      if (max === 0) return settle(call, 0, []);
      read(call, Math.min(max, READ_CHUNK));
    }),
    netSocketWrite: (id, data) => start("write", id, "tcp", 0, (call, state) => {
      if (state.writeShut || state.handle.writableEnded) return settle(call, -1, null, failure("ECLOSED", "socket write side is shut down"));
      if (!data.length) return settle(call, 0, []);
      call.cancel = error => settle(call, -1, null, error);
      state.handle.write(Buffer.from(data), error => error ? settle(call, -1, null, io(error)) : settle(call, data.length));
    }),
    netSocketSend: (id, host, port, data, timeout) => start("send", id, "udp", timeout, (call, state) => {
      if (!textValid(host) || port < 0 || port > 65535 || data.length > 65507) return settle(call, -1, null, failure("EINVAL", "invalid datagram endpoint or size"));
      call.cancel = error => settle(call, -1, null, error);
      state.handle.send(Buffer.from(data), port, host, error => error ? settle(call, -1, null, io(error)) : settle(call, data.length));
    }),
    netSocketReceive: (id, max, timeout) => start("receive", id, "udp", timeout, call => {
      if (max < 0 || max > 65535) return settle(call, -1, null, failure("EINVAL", "invalid datagram read size"));
      receive(call, max);
    }),
    netSocketShutdown: id => control(id, "tcp", state => {
      if ([...state.calls].some(call => call.kind === "write")) return failure("EBUSY", "write is pending");
      state.writeShut = true;
      if (!state.handle.writableEnded) state.handle.end();
      return null;
    }),
    netSocketOption: (id, option, value) => control(id, option === OPTION_NODELAY ? "tcp" : "udp", state => {
      if (option === OPTION_NODELAY) state.handle.setNoDelay(value !== 0);
      else if (option === OPTION_BROADCAST) state.handle.setBroadcast(value !== 0);
      else if (option === OPTION_TTL && value >= 1 && value <= 255) state.handle.setTTL(value);
      else return failure("EINVAL", "unsupported socket option");
      return null;
    }),
    netSocketCallWait: id => {
      const call = calls.get(id);
      if (!call || call.done) return Promise.resolve(0);
      return new Promise(resolve => call.waiters.push(resolve));
    },
    netSocketCallValue: id => {
      const call = calls.get(id);
      if (!call?.done) return -1n;
      call.claimed = true;
      return call.code ? -1n : call.value;
    },
    netSocketCallBytes: id => {
      const call = calls.get(id);
      if (!call?.done) return [];
      call.claimed = true;
      return call.code ? [] : call.bytes;
    },
    netSocketCallHost: id => calls.get(id)?.peer?.host ?? "",
    netSocketCallPort: id => calls.get(id)?.peer?.port ?? 0,
    netSocketCallCode: id => {
      const call = calls.get(id);
      if (!call) return "ELIMIT";
      return call.done ? call.code : "";
    },
    netSocketCallMessage: id => calls.get(id)?.message ?? "socket call capacity exhausted",
    netSocketCallCancel: id => { const call = calls.get(id); if (call && !call.done) call.cancel?.(cancelled()); },
    netSocketCallRelease: id => {
      const call = calls.get(id);
      if (!call) return;
      calls.delete(id);
      call.released = true;
      if (!call.done) call.cancel?.(cancelled());
      else dispose(call);
    },
    netSocketAddress: (id, peer) => { const state = sockets.get(id); return state ? (peer ? state.peer : state.local).host : ""; },
    netSocketPort: (id, peer) => { const state = sockets.get(id); return state ? (peer ? state.peer : state.local).port : 0; },
    netSocketClose: close,
  };
}
