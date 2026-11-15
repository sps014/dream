import assert from 'node:assert/strict';
import net from 'node:net';
import test from 'node:test';
import { setNodeNet } from '../runtime/src/platform.js';
import { makeSocketHost } from '../runtime/src/hosts/sockets.js';

test('deferred socket writes snapshot borrowed input before returning', async () => {
  setNodeNet(net);
  const host = makeSocketHost();
  let peer;
  let received;
  const data = new Promise(resolve => { received = resolve; });
  const server = net.createServer(socket => {
    peer = socket;
    socket.once('data', bytes => received(bytes.toString()));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let socket;
  try {
    const opening = host.netSocketStart(JSON.stringify({action: 'connect', host: '127.0.0.1', port: server.address().port, timeout_ms: 1000}), []);
    await host.netSocketWait(opening);
    const metadata = host.netSocketResult(opening);
    assert.equal(metadata[0], 1);
    socket = JSON.parse(new TextDecoder().decode(Uint8Array.from(metadata.slice(1)))).id;
    host.netSocketClaim(opening);
    host.netSocketRelease(opening);
    const input = new TextEncoder().encode('abc');
    const writing = host.netSocketStart(JSON.stringify({action: 'write', id: socket, timeout_ms: 1000}), input);
    input.fill(0);
    await host.netSocketWait(writing);
    assert.equal(host.netSocketResult(writing)[0], 1);
    assert.equal(await data, 'abc');
    host.netSocketRelease(writing);
  } finally {
    if (socket) host.netSocketClose(socket);
    peer?.destroy();
    await new Promise(resolve => server.close(resolve));
  }
});
