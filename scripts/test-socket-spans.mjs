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
    const opening = host.netSocketConnect('127.0.0.1', server.address().port, 1000);
    await host.netSocketCallWait(opening);
    assert.equal(host.netSocketCallCode(opening), '');
    socket = Number(host.netSocketCallValue(opening));
    host.netSocketCallRelease(opening);
    const input = new TextEncoder().encode('abc');
    const writing = host.netSocketWrite(socket, input);
    input.fill(0);
    await host.netSocketCallWait(writing);
    assert.equal(host.netSocketCallCode(writing), '');
    assert.equal(await data, 'abc');
    host.netSocketCallRelease(writing);
  } finally {
    if (socket) host.netSocketClose(socket);
    peer?.destroy();
    await new Promise(resolve => server.close(resolve));
  }
});
