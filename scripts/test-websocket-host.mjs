import test from "node:test";
import assert from "node:assert/strict";
import { makeSocketHost } from "../runtime/src/hosts/sockets.js";
import { WIRE_VERSION, WireReader, WireWriter } from "../runtime/src/wire.js";

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
    const CONNECT = 1, RECEIVE = 2, SEND_TEXT = 3, SEND_BINARY = 4, CLOSE = 5;
    const start = (action, id = 0, bytes = [], args = writer => writer) =>
      host.netWsStart(args(new WireWriter().u8(WIRE_VERSION).u8(action).i32(id).i32(0)).finish(), bytes);
    const connect = url => start(CONNECT, 0, [], writer => writer.text(url));
    const result = async op => {
      await host.netSocketWait(op);
      const wire = host.netSocketResult(op);
      host.netSocketClaim(op); host.netSocketRelease(op);
      return {status:wire[0], reader:new WireReader(wire.slice(1))};
    };
    const code = async op => {
      const {status, reader} = await result(op);
      assert.equal(status, 2);
      return reader.text();
    };
    const id = async op => {
      const {status, reader} = await result(op);
      assert.equal(status, 1);
      return reader.i32();
    };
    const event = async op => {
      const {reader} = await result(op);
      const kind = reader.u8();
      if (kind === 1) return {kind, text:reader.text()};
      if (kind === 2) return {kind, bytes:Array.from(reader.bytesField())};
      return {kind, code:reader.i32(), reason:reader.text()};
    };
    const socket = await id(connect("ws://localhost/echo"));
    assert.equal((await result(start(SEND_TEXT, socket, new TextEncoder().encode("hello")))).status, 1);
    assert.deepEqual(await event(start(RECEIVE, socket)), {kind:1, text:"hello"});
    assert.equal((await result(start(SEND_BINARY, socket, [0,255]))).status, 1);
    assert.deepEqual(await event(start(RECEIVE, socket)), {kind:2, bytes:[0,255]});
    assert.equal((await result(start(CLOSE, socket, [], writer => writer.i32(1000).text("done")))).status, 1);
    assert.deepEqual(await event(start(RECEIVE, socket)), {kind:3, code:1000, reason:"done"});
    host.netWsClose(socket);
    const second = await id(connect("ws://localhost/echo"));
    const pending = start(RECEIVE, second);
    await new Promise(resolve => setImmediate(resolve));
    host.netSocketCancel(pending);
    assert.equal(await code(pending), "ECANCELLED");
    assert.equal(await code(start(SEND_TEXT, second, [1])), "ECLOSED");
    const third = await id(connect("ws://localhost/echo"));
    const early = start(RECEIVE, third);
    host.netSocketCancel(early);
    assert.equal(await code(early), "ECANCELLED");
    assert.equal(await code(start(SEND_TEXT, third, [1])), "ECLOSED");
    assert.equal(await code(connect("http://localhost")), "EINVAL");
    assert.equal(await code(host.netWsStart([WIRE_VERSION + 1, RECEIVE, 0, 0, 0, 0, 0, 0, 0, 0], [])), "EINVAL");
  } finally { globalThis.WebSocket = original; }
});
