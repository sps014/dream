import { isNode, getNodeNet } from "../platform.js";

import { makeWebSocketService } from "./websocket.js";
import { wireFailure } from "../wire.js";

export function makeSocketHost() {
  const websocket = makeWebSocketService();
  const resources = new Map();
  const operations = new Map();
  let nextResource = 0, nextOperation = 0;
  const encoder = new TextEncoder();
  const fail = (code, message) => Object.assign(new Error(message), {code});
  const cancelled = () => fail("ECANCELLED", "socket operation cancelled");
  const check = signal => { if (signal.aborted) throw signal.reason || cancelled(); };
  function endpoint(request) {
    if (typeof request.host !== "string" || !request.host || request.host.includes("\0") || !Number.isInteger(request.port) || request.port < 0 || request.port > 65535) throw fail("EINVAL", "invalid socket endpoint");
    return {host:request.host,port:request.port};
  }
  function wait(emitter, names, signal) {
    check(signal);
    return new Promise((resolve,reject) => {
      const cleanup = () => { for (const name of names) emitter.off(name, done); emitter.off("error",error); signal.removeEventListener("abort",abort); };
      const done = () => { cleanup(); resolve(); };
      const error = value => { cleanup(); reject(value); };
      const abort = () => error(signal.reason || cancelled());
      for (const name of names) emitter.once(name,done);
      emitter.once("error",error); signal.addEventListener("abort",abort,{once:true});
    });
  }
  function close(id) {
    const state = resources.get(id);
    if (!state) return;
    resources.delete(id); state.closed = true;
    if (state.kind === "listener") {
      for (const pending of state.pending.splice(0)) close(pending);
      state.socket.emit("dream-ready");
      try { state.socket.close(); } catch (_) {}
    } else if (state.kind === "udp") {
      state.pending.length = 0; state.socket.emit("dream-ready");
      try { state.socket.close(); } catch (_) {}
    } else state.socket.destroy();
  }
  function track(socket, kind) {
    if (resources.size >= 1024 || nextResource === 2147483647) throw fail("ELIMIT", "socket capacity exhausted");
    const state = {id:++nextResource,socket,kind,closed:false,ended:false,error:null,reading:false,writing:false,pending:[]};
    socket.on("error",error => { state.error = error; socket.emit("dream-ready"); });
    socket.on("end",() => { state.ended = true; });
    socket.on("close",() => { state.closed = true; socket.emit("dream-ready"); });
    const local = socket.address();
    state.metadata = {id:state.id,local_host:local.address,local_port:local.port,peer_host:socket.remoteAddress || null,peer_port:socket.remotePort || null};
    resources.set(state.id,state);
    return state;
  }
  const metadata = state => ({bytes:encoder.encode(JSON.stringify(state.metadata)),owned:state.id});
  function get(id) { const state = resources.get(id); if (!state) throw fail("ECLOSED","socket is closed"); return state; }
  async function create(request, signal) {
    const address = endpoint(request);
    if (request.action === "udp_bind") {
      const dgram = await import("node:dgram"); check(signal);
      const socket = dgram.createSocket(address.host.includes(":") ? "udp6" : "udp4");
      socket.on("error",() => {});
      try {
        const ready = wait(socket,["listening"],signal); socket.bind(address.port,address.host); await ready; check(signal);
        const state = track(socket,"udp");
        socket.on("message",(bytes,peer) => { if (state.pending.length < 64) { state.pending.push({bytes:Uint8Array.from(bytes),peer}); socket.emit("dream-ready"); } });
        return metadata(state);
      } catch (error) { try { socket.close(); } catch (_) {} throw error; }
    }
    const net = getNodeNet(); if (!net) throw fail("EUNSUPPORTED","TCP host unavailable");
    if (request.action === "connect") {
      const socket = new net.Socket({allowHalfOpen:true}); socket.on("error",() => {});
      try { const ready = wait(socket,["connect"],signal); socket.connect(address.port,address.host); await ready; check(signal); socket.pause(); return metadata(track(socket,"tcp")); }
      catch (error) { socket.destroy(); throw error; }
    }
    const server = net.createServer({pauseOnConnect:true,allowHalfOpen:true}); server.on("error",() => {});
    try {
      const ready = wait(server,["listening"],signal); server.listen({host:address.host,port:address.port}); await ready; check(signal);
      const state = track(server,"listener");
      server.on("connection",socket => {
        if (state.closed || state.pending.length >= 64) { socket.destroy(); return; }
        try { const child = track(socket,"tcp"); state.pending.push(child.id); server.emit("dream-ready"); } catch (_) { socket.destroy(); }
      });
      return metadata(state);
    } catch (error) { try { server.close(); } catch (_) {} throw error; }
  }
  async function execute(request, bytes, signal) {
    if (!isNode) throw fail("EUNSUPPORTED","raw sockets are unavailable in browsers");
    check(signal);
    if (["connect","listen","udp_bind"].includes(request.action)) return create(request,signal);
    const state = get(request.id);
    if (request.action === "accept" && state.kind === "listener") {
      while (true) {
        check(signal); if (state.closed) throw fail("ECLOSED","listener is closed");
        if (state.error) throw state.error;
        if (state.pending.length) { const id = state.pending.shift(); const child = resources.get(id); if (child && !child.closed) return metadata(child); close(id); continue; }
        await wait(state.socket,["dream-ready"],signal);
      }
    }
    if (request.action === "read" && state.kind === "tcp") {
      const count = request.count;
      if (!Number.isInteger(count) || count < 0 || count > 16777216) throw fail("EINVAL","invalid TCP read size");
      if (!count) return {bytes:new Uint8Array()};
      if (state.reading) throw fail("EBUSY","socket already has a pending read");
      state.reading = true;
      try {
        while (true) {
          check(signal);
          if (state.socket.readableLength) return {bytes:Uint8Array.from(state.socket.read(Math.min(count,state.socket.readableLength)))};
          if (state.error) throw state.error;
          if (state.ended) return {bytes:new Uint8Array()};
          if (state.closed) throw fail("ECLOSED","socket is closed");
          await wait(state.socket,["readable","end","close"],signal);
        }
      } finally { state.reading = false; }
    }
    if (request.action === "write" && state.kind === "tcp") {
      if (bytes.length > 16777216) throw fail("EINVAL","TCP writes may not exceed 16 MiB");
      if (state.writing) throw fail("EBUSY","socket already has a pending write");
      if (state.socket.writableEnded) throw fail("ECLOSED","socket write side is shut down");
      if (!bytes.length) return {bytes:encoder.encode("0")};
      state.writing = true;
      try {
        await new Promise((resolve,reject) => {
          const abort = () => { signal.removeEventListener("abort",abort); reject(signal.reason || cancelled()); };
          signal.addEventListener("abort",abort,{once:true});
          state.socket.write(bytes,error => { signal.removeEventListener("abort",abort); error ? reject(error) : resolve(); });
        });
        return {bytes:encoder.encode(String(bytes.length))};
      } finally { state.writing = false; }
    }
    if (request.action === "udp_send" && state.kind === "udp") {
      const address = endpoint(request);
      if (bytes.length > 65507) throw fail("EINVAL","datagram exceeds portable UDP size limit");
      await new Promise((resolve,reject) => {
        const abort = () => { signal.removeEventListener("abort",abort); reject(signal.reason || cancelled()); };
        signal.addEventListener("abort",abort,{once:true});
        state.socket.send(bytes,address.port,address.host,error => { signal.removeEventListener("abort",abort); error ? reject(error) : resolve(); });
      });
      check(signal); return {bytes:encoder.encode(String(bytes.length))};
    }
    if (request.action === "udp_receive" && state.kind === "udp") {
      const count = request.count;
      if (!Number.isInteger(count) || count < 0 || count > 65535) throw fail("EINVAL","invalid datagram read size");
      if (state.reading) throw fail("EBUSY","datagram receive is already pending");
      state.reading = true;
      try {
        while (true) {
          check(signal); if (state.closed) throw fail("ECLOSED","socket is closed"); if (state.error) throw state.error;
          if (state.pending.length) {
            const {bytes,peer} = state.pending.shift();
            return {bytes:encoder.encode(JSON.stringify({host:peer.address,port:peer.port,bytes:Array.from(bytes.subarray(0,count),b=>b.toString(16).padStart(2,"0")).join(""),truncated:bytes.length>count}))};
          }
          await wait(state.socket,["dream-ready"],signal);
        }
      } finally { state.reading = false; }
    }
    throw fail("EINVAL","operation does not match socket kind");
  }
  function start(text, input, run = execute, dispose = close, parse = JSON.parse) {
      if (nextOperation === 2147483647 || operations.size >= 4096) return 0;
      const bytes = Uint8Array.from(input || []);
      const raw = typeof text === "string" ? text : Uint8Array.from(text || []);
      const id = ++nextOperation, abort = new AbortController();
      let done;
      const state = {abort,result:null,owned:0,dispose,released:false,done:new Promise(resolve=>{done=resolve;})};
      operations.set(id,state);
      Promise.resolve().then(async () => {
        let timer;
        try {
          const request = parse(raw);
          if (!Number.isInteger(request.timeout_ms) || request.timeout_ms < 0 || request.timeout_ms > 2147483647) throw fail("EINVAL","invalid socket timeout");
          if (request.timeout_ms) timer = setTimeout(()=>abort.abort(fail("ETIMEOUT","socket operation deadline exceeded")),request.timeout_ms);
          const result = await run(request,bytes,abort.signal);
          state.owned = result.owned || 0;
          if (abort.signal.aborted) throw abort.signal.reason || cancelled();
          state.result = [1,...result.bytes];
        } catch (error) {
          state.result = wireFailure(["EINVAL","ECLOSED","ECANCELLED","ETIMEOUT","EUNSUPPORTED","EBUSY","ELIMIT"].includes(error.code)?error.code:"EIO",error.message||"socket operation failed");
        } finally {
          clearTimeout(timer);
          if (state.released && state.owned) dispose(state.owned);
          done();
        }
      });
      return id;
  }
  return {
    netSocketStart: (text, input) => start(text, input),
    netWsStart: (request, input) => start(request, input, websocket.execute, websocket.close, websocket.parse),
    netWsClose: id => websocket.close(id),
    async netSocketWait(id) { const state = operations.get(id); if (state) await state.done; return 0; },
    netSocketResult: id => operations.get(id)?.result || [0],
    netSocketCancel: id => operations.get(id)?.abort.abort(cancelled()),
    netSocketRelease: id => { const state = operations.get(id); if (state) { state.released=true; state.abort.abort(cancelled()); if (state.owned) state.dispose(state.owned); operations.delete(id); } },
    netSocketClaim: id => { const state = operations.get(id); if (state) state.owned=0; },
    netSocketClose: id => close(id),
    netSocketControl: (id,text) => {
      try {
        if (!isNode) throw fail("EUNSUPPORTED","raw sockets are unavailable in browsers");
        const state = get(id), request = JSON.parse(text);
        if (request.action === "shutdown_write" && state.kind === "tcp") {
          if (state.writing) throw fail("EBUSY","write is pending");
          if (!state.socket.writableEnded) state.socket.end();
        } else if (request.action === "nodelay" && state.kind === "tcp" && typeof request.value === "boolean") { if (state.reading) throw fail("EBUSY","read is pending"); state.socket.setNoDelay(request.value); }
        else if (request.action === "broadcast" && state.kind === "udp" && typeof request.value === "boolean") state.socket.setBroadcast(request.value);
        else if (request.action === "ttl" && state.kind === "udp" && Number.isInteger(request.value) && request.value>0 && request.value<=255) state.socket.setTTL(request.value);
        else throw fail("EINVAL","unsupported socket option");
        return "{}";
      } catch (error) { return JSON.stringify({code:error.code||"EIO",message:error.message}); }
    },
  };
}
