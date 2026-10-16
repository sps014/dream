export function makeWebSocketService() {
  const sockets = new Map();
  const encoder = new TextEncoder();
  let next = 0;
  const fail = (code, message) => Object.assign(new Error(message), {code});
  const encode = value => encoder.encode(JSON.stringify(value));
  function close(id) {
    const state = sockets.get(id);
    if (!state) return;
    sockets.delete(id);
    state.closed = true;
    state.socket.close();
    state.wake?.();
  }
  function check(signal) { if (signal.aborted) throw signal.reason || fail("ECANCELLED", "WebSocket operation cancelled"); }
  async function execute(request, bytes, signal) {
    if (signal.aborted) close(request.id);
    check(signal);
    if (request.action === "connect") {
      if (typeof globalThis.WebSocket !== "function") throw fail("EUNSUPPORTED", "WebSocket requires a browser or Node 22+");
      let url;
      try { url = new URL(request.url); } catch (_) { throw fail("EINVAL", "invalid WebSocket URL"); }
      if (!["ws:", "wss:"].includes(url.protocol)) throw fail("EINVAL", "expected ws:// or wss:// URL");
      if (sockets.size >= 1024 || next === 2147483647) throw fail("ELIMIT", "WebSocket capacity exhausted");
      const socket = new WebSocket(url.href);
      socket.binaryType = "arraybuffer";
      const state = {id:++next, socket, queue:[], closed:false, reading:false, error:null, wake:null};
      sockets.set(state.id, state);
      socket.addEventListener("message", event => {
        if (state.queue.length >= 1024) { state.error = fail("ELIMIT", "WebSocket message queue exceeded"); close(state.id); return; }
        state.queue.push(typeof event.data === "string" ? {kind:"text",text:event.data} : {kind:"binary",bytes:Array.from(new Uint8Array(event.data))});
        state.wake?.();
      });
      socket.addEventListener("close", event => { state.closed = true; state.queue.push({kind:"close",code:event.code,reason:event.reason}); state.wake?.(); });
      socket.addEventListener("error", () => { state.error = fail("EIO", "WebSocket transport error"); state.wake?.(); });
      const abort = () => close(state.id);
      signal.addEventListener("abort", abort, {once:true});
      try {
        await new Promise((resolve, reject) => {
          const cleanup = () => { socket.removeEventListener("open", opened); socket.removeEventListener("error", errored); socket.removeEventListener("close", errored); signal.removeEventListener("abort", aborted); };
          const opened = () => { cleanup(); resolve(); };
          const errored = () => { cleanup(); reject(state.error || fail("EIO", "WebSocket handshake failed")); };
          const aborted = () => { cleanup(); reject(signal.reason || fail("ECANCELLED", "WebSocket connection cancelled")); };
          socket.addEventListener("open", opened, {once:true}); socket.addEventListener("error", errored, {once:true}); socket.addEventListener("close", errored, {once:true}); signal.addEventListener("abort", aborted, {once:true});
          if (signal.aborted) aborted();
        });
        check(signal);
        return {bytes:encode({id:state.id}),owned:state.id};
      } catch (error) { close(state.id); throw error; }
      finally { signal.removeEventListener("abort", abort); }
    }
    const state = sockets.get(request.id);
    if (!state) throw fail("ECLOSED", "WebSocket is closed");
    const abort = () => close(state.id);
    signal.addEventListener("abort", abort, {once:true});
    try {
      if (request.action === "receive") {
        if (state.reading) throw fail("EBUSY", "WebSocket receive already pending");
        state.reading = true;
        try {
          while (!state.queue.length) {
            check(signal);
            if (state.error) throw state.error;
            if (state.closed) return {bytes:encode({kind:"close",code:1006,reason:""})};
            await new Promise(resolve => { state.wake = resolve; if (signal.aborted || state.closed || state.error || state.queue.length) resolve(); });
          }
          check(signal);
          return {bytes:encode(state.queue.shift())};
        } finally { state.reading = false; state.wake = null; }
      }
      check(signal);
      if (state.closed || state.socket.readyState !== WebSocket.OPEN) throw fail("ECLOSED", "WebSocket is closed");
      if (request.action === "send_text") state.socket.send(new TextDecoder("utf-8", {fatal:true}).decode(bytes));
      else if (request.action === "send_binary") state.socket.send(bytes);
      else if (request.action === "close") {
        const code = request.code, reason = request.reason;
        if (!Number.isInteger(code) || (code !== 1000 && (code < 3000 || code > 4999)) || typeof reason !== "string" || encoder.encode(reason).length > 123) throw fail("EINVAL", "invalid WebSocket close frame");
        state.socket.close(code, reason);
      } else throw fail("EINVAL", "unknown WebSocket action");
      return {bytes:[]};
    } finally { signal.removeEventListener("abort", abort); }
  }
  return {execute, close};
}
