import test from "node:test";
import assert from "node:assert/strict";
import * as dgram from "node:dgram";
import * as net from "node:net";
import { setNodeDgram, setNodeNet } from "../runtime/src/platform.js";
import { makeSocketHost } from "../runtime/src/hosts/sockets.js";

setNodeNet(net);
setNodeDgram(dgram);
const host = makeSocketHost();
const encoder = new TextEncoder(), decoder = new TextDecoder();

// Waits for a call and returns its result the way the guest's SocketCall reads it.
async function finish(call) {
  await host.netSocketCallWait(call);
  const code = host.netSocketCallCode(call);
  const result = code ? { code, message: host.netSocketCallMessage(call) } : {
    value: Number(host.netSocketCallValue(call)),
    bytes: Uint8Array.from(host.netSocketCallBytes(call)),
    host: host.netSocketCallHost(call),
    port: host.netSocketCallPort(call),
  };
  host.netSocketCallRelease(call);
  return result;
}
async function ok(call) {
  const result = await finish(call);
  assert.equal(result.code, undefined, result.message);
  return result;
}

test("TCP supports half-close, EOF, cancellation, deadlines and listener cleanup", async () => {
  const listener = (await ok(host.netSocketListen("127.0.0.1", 0))).value;
  const port = host.netSocketPort(listener, false);
  assert.ok(port > 0);
  const accepting = host.netSocketAccept(listener, 1000);
  const client = (await ok(host.netSocketConnect("127.0.0.1", port, 1000))).value;
  const server = (await ok(accepting)).value;
  try {
    assert.equal(host.netSocketPort(client, true), port);
    assert.equal((await ok(host.netSocketWrite(client, encoder.encode("hello")))).value, 5);
    assert.equal((await finish(host.netSocketShutdown(client))).code, undefined);
    assert.equal(decoder.decode((await ok(host.netSocketRead(server, 100, 1000))).bytes), "hello");
    assert.equal((await ok(host.netSocketRead(server, 100, 1000))).bytes.length, 0);
    await ok(host.netSocketWrite(server, encoder.encode("reply")));
    await finish(host.netSocketShutdown(server));
    assert.equal((await finish(host.netSocketWrite(server, [1]))).code, "ECLOSED");
    assert.equal(decoder.decode((await ok(host.netSocketRead(client, 100, 1000))).bytes), "reply");
    assert.equal((await ok(host.netSocketRead(client, 100, 1000))).bytes.length, 0);
    const pending = host.netSocketAccept(listener, 0);
    assert.equal((await finish(host.netSocketAccept(listener, 0))).code, "EBUSY");
    host.netSocketCallCancel(pending);
    assert.equal((await finish(pending)).code, "ECANCELLED");
    assert.equal((await finish(host.netSocketAccept(listener, 5))).code, "ETIMEOUT");
    const closing = host.netSocketAccept(listener, 0);
    host.netSocketClose(listener);
    assert.equal((await finish(closing)).code, "ECLOSED");
    assert.equal((await finish(host.netSocketRead(listener, 1, 0))).code, "ECLOSED");
  } finally { for (const id of [client, server, listener]) host.netSocketClose(id); }
});

test("UDP preserves empty messages, reports truncation, and wakes closed receivers", async () => {
  const first = (await ok(host.netSocketBind("127.0.0.1", 0))).value;
  const second = (await ok(host.netSocketBind("127.0.0.1", 0))).value;
  const port = host.netSocketPort(second, false);
  try {
    assert.equal((await ok(host.netSocketSend(first, "127.0.0.1", port, encoder.encode("abcdef"), 1000))).value, 6);
    const message = await ok(host.netSocketReceive(second, 3, 1000));
    assert.equal(decoder.decode(message.bytes), "abc");
    assert.equal(message.value, 6);
    assert.equal(message.port, host.netSocketPort(first, false));
    await ok(host.netSocketSend(first, "127.0.0.1", port, [], 1000));
    const empty = await ok(host.netSocketReceive(second, 3, 1000));
    assert.equal(empty.bytes.length, 0);
    assert.equal(empty.value, 0);
    assert.equal((await finish(host.netSocketOption(first, 2, 0))).code, "EINVAL");
    assert.equal((await finish(host.netSocketOption(first, 2, 64))).code, undefined);
    const pending = host.netSocketReceive(second, 10, 0);
    host.netSocketClose(second);
    assert.equal((await finish(pending)).code, "ECLOSED");
  } finally { host.netSocketClose(first); host.netSocketClose(second); }
});
