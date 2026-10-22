import assert from "node:assert/strict";
import http from "node:http";
import test from "node:test";
import { makeHttpHost } from "../runtime/src/hosts/http.js";
import { WireReader } from "../runtime/src/wire.js";

async function awaitWire(host, id) {
  let timer;
  try {
    await Promise.race([
      host.netOperationWait(id),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error("HTTP operation did not complete")), 5000); }),
    ]);
    const wire = host.netOperationPoll(id);
    assert.notEqual(wire[0], 0, "completion notification must precede readiness");
    if (wire[0] === 1) host.netOperationClaim(id);
    host.netOperationRelease(id);
    return wire;
  } finally { clearTimeout(timer); }
}
function decode(wire) {
  if (wire[0] === 2) { const reader = new WireReader(wire.slice(1)); return { code: reader.text(), message: reader.text() }; }
  return JSON.parse(new TextDecoder().decode(Uint8Array.from(wire.slice(1))));
}

test("Fetch streams, redirects, deadlines, repeated headers and operation disposal", async () => {
  const sockets = new Set();
  let accepted = 0;
  const server = http.createServer((req, res) => {
    if (req.url === "/redirect") { res.writeHead(302, { location: "/stream" }); res.end(); return; }
    if (req.url === "/loop") { res.writeHead(302, { location: "/loop" }); res.end(); return; }
    if (req.url === "/slow") { setTimeout(() => res.end("late"), 100); return; }
    if (req.url === "/stream") {
      res.writeHead(200, { "set-cookie": ["a=1", "b=2"] });
      res.write("first");
      setTimeout(() => res.end("second"), 15);
      return;
    }
    const body = [];
    req.on("data", chunk => body.push(chunk));
    req.on("end", () => { res.writeHead(404, { "x-method": req.method }); res.end(Buffer.concat(body)); });
  });
  server.on("connection", socket => { accepted++; sockets.add(socket); socket.on("close", () => sockets.delete(socket)); });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  const base = `http://127.0.0.1:${server.address().port}`;
  const host = makeHttpHost();
  const client = host.netClientCreate();
  const request = (path, options = {}, bytes = []) => host.netHttpStart(client, JSON.stringify({ method: "GET", url: base + path, headers: [], timeout_ms: 3000, connect_timeout_ms: 0, max_redirects: 5, ...options }), bytes);
  try {
    for (const method of ["POST", "PUT", "PATCH", "DELETE"]) {
      const wire = await awaitWire(host, request("/echo", { method, headers: [["x-repeat", "a"], ["x-repeat", "b"]] }, [65, 66]));
      assert.equal(wire[0], 1);
      const response = decode(wire);
      assert.equal(response.status, 404);
      assert.deepEqual(response.headers.find(([name]) => name === "x-method"), ["x-method", method]);
      const bytes = await awaitWire(host, host.netBodyRead(response.body, 1));
      assert.deepEqual(bytes, [1, 65]);
      assert.deepEqual(await awaitWire(host, host.netBodyRead(response.body, 10)), [1, 66]);
      assert.deepEqual(await awaitWire(host, host.netBodyRead(response.body, 1)), [1]);
      host.netBodyClose(response.body);
      host.netBodyClose(response.body);
    }
    const stream = decode(await awaitWire(host, request("/redirect")));
    assert.equal(stream.status, 200);
    assert.ok(stream.url.endsWith("/stream"));
    assert.equal(stream.headers.filter(([key]) => key === "set-cookie").length, 2);
    let text = "";
    while (true) {
      const wire = await awaitWire(host, host.netBodyRead(stream.body, 2));
      assert.equal(wire[0], 1);
      if (wire.length === 1) break;
      text += new TextDecoder().decode(Uint8Array.from(wire.slice(1)));
    }
    assert.equal(text, "firstsecond");
    host.netBodyClose(stream.body);
    const redirect = decode(await awaitWire(host, request("/redirect", { max_redirects: 0 })));
    assert.equal(redirect.status, 302);
    host.netBodyClose(redirect.body);
    assert.equal(decode(await awaitWire(host, request("/loop", { max_redirects: 1 }))).code, "EREDIRECT");
    assert.equal(decode(await awaitWire(host, request("/slow", { timeout_ms: 10 }))).code, "ETIMEOUT");
    assert.equal(decode(await awaitWire(host, request("/echo", { connect_timeout_ms: 10 }))).code, "ENOTSUP");
    assert.equal(decode(await awaitWire(host, request("/echo", { headers: [["bad\nname", "value"]] }))).code, "EINVAL");
    const cancelled = request("/slow");
    const abandonedWait = host.netOperationWait(cancelled);
    host.netOperationRelease(cancelled);
    await abandonedWait;
    assert.equal(decode(host.netOperationPoll(cancelled)).code, "ECLOSED");
    const orphaned = request("/stream");
    let orphanWire;
    while (true) {
      orphanWire = host.netOperationPoll(orphaned);
      if (orphanWire[0]) break;
      await new Promise(resolve => setTimeout(resolve, 1));
    }
    const orphanBody = decode(orphanWire).body;
    host.netOperationRelease(orphaned);
    assert.equal(decode(await awaitWire(host, host.netBodyRead(orphanBody, 1))).code, "ECLOSED");
    const pending = request("/slow");
    await new Promise(resolve => setTimeout(resolve, 5));
    host.netClientClose(client);
    assert.equal(decode(await awaitWire(host, pending)).code, "ECLOSED");
    assert.ok(accepted < 16, `expected pooled connections, accepted ${accepted}`);
  } finally {
    host.netClientClose(client);
    for (const socket of sockets) socket.destroy();
    await new Promise(resolve => server.close(resolve));
  }
});
