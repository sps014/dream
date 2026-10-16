import test from "node:test";
import assert from "node:assert/strict";
import { makeSocketHost } from "../runtime/src/hosts/sockets.js";

class EchoWebSocket extends EventTarget {
  static OPEN = 1;
  readyState = 0;
  constructor(url) {
    super();
    assert.match(url, /^wss?:/);
    queueMicrotask(() => { this.readyState = 1; this.dispatchEvent(new Event("open")); });
  }
  send(data) {
    const event = new Event("message");
    event.data = typeof data === "string" ? data : Uint8Array.from(data).buffer;
    queueMicrotask(() => this.dispatchEvent(event));
  }
  close(code = 1000, reason = "") {
    this.readyState = 3;
    const event = Object.assign(new Event("close"), {code, reason});
    queueMicrotask(() => this.dispatchEvent(event));
  }
}

test("WebSocket hosts share operation ownership, close messages and cancellation", async () => {
  const original = globalThis.WebSocket;
  globalThis.WebSocket = EchoWebSocket;
  try {
    const host = makeSocketHost();
    const start = (action, id = 0, bytes = [], extra = {}) => host.netWsStart(JSON.stringify({action, id, timeout_ms:0, ...extra}), bytes);
    const result = async op => {
      await host.netSocketWait(op);
      const wire = host.netSocketResult(op);
      host.netSocketClaim(op); host.netSocketRelease(op);
      return {status:wire[0], value:wire.length > 1 ? JSON.parse(new TextDecoder().decode(Uint8Array.from(wire.slice(1)))) : null};
    };
    const connection = await result(start("connect", 0, [], {url:"ws://localhost/echo"}));
    assert.equal(connection.status, 1);
    const id = connection.value.id;
    assert.equal((await result(start("send_text", id, new TextEncoder().encode("hello")))).status, 1);
    assert.deepEqual((await result(start("receive", id))).value, {kind:"text", text:"hello"});
    assert.equal((await result(start("send_binary", id, [0,255]))).status, 1);
    assert.deepEqual((await result(start("receive", id))).value, {kind:"binary", bytes:[0,255]});
    assert.equal((await result(start("close", id, [], {code:1000,reason:"done"}))).status, 1);
    assert.deepEqual((await result(start("receive", id))).value, {kind:"close", code:1000, reason:"done"});
    host.netWsClose(id);
    const second = (await result(start("connect", 0, [], {url:"ws://localhost/echo"}))).value.id;
    const pending = start("receive", second);
    await new Promise(resolve => setImmediate(resolve));
    host.netSocketCancel(pending);
    assert.equal((await result(pending)).value.code, "ECANCELLED");
    assert.equal((await result(start("send_text", second, [1]))).value.code, "ECLOSED");
    const third = (await result(start("connect", 0, [], {url:"ws://localhost/echo"}))).value.id;
    const early = start("receive", third);
    host.netSocketCancel(early);
    assert.equal((await result(early)).value.code, "ECANCELLED");
    assert.equal((await result(start("send_text", third, [1]))).value.code, "ECLOSED");
    assert.equal((await result(start("connect", 0, [], {url:"http://localhost"}))).value.code, "EINVAL");
  } finally { globalThis.WebSocket = original; }
});
