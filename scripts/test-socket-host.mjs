import test from "node:test";
import assert from "node:assert/strict";
import * as net from "node:net";
import {setNodeNet} from "../runtime/src/platform.js";
import {makeSocketHost} from "../runtime/src/hosts/sockets.js";
setNodeNet(net);
const host=makeSocketHost();
const encoder=new TextEncoder(), decoder=new TextDecoder();
const start=(request,bytes=[])=>host.netSocketStart(JSON.stringify({timeout_ms:1000,...request}),bytes);
async function complete(id,claim=false) {
  await host.netSocketWait(id);
  const wire=host.netSocketResult(id);
  if (claim) host.netSocketClaim(id);
  host.netSocketRelease(id);
  if (wire[0]!==1) throw Object.assign(new Error(decoder.decode(Uint8Array.from(wire.slice(1)))),{wire});
  return Uint8Array.from(wire.slice(1));
}
async function create(action) { return JSON.parse(decoder.decode(await complete(start({action,host:'127.0.0.1',port:0}),true))); }
test('TCP supports half-close, EOF, cancellation, deadlines and listener cleanup',async()=>{
 const listener=await create('listen');
 const accepting=start({action:'accept',id:listener.id});
 const client=JSON.parse(decoder.decode(await complete(start({action:'connect',host:'127.0.0.1',port:listener.local_port}),true)));
 const server=JSON.parse(decoder.decode(await complete(accepting,true)));
 try {
  await complete(start({action:'write',id:client.id},encoder.encode('hello')));
  assert.equal(host.netSocketControl(client.id,JSON.stringify({action:'shutdown_write'})),'{}');
  assert.equal(decoder.decode(await complete(start({action:'read',id:server.id,count:100}))),'hello');
  assert.equal((await complete(start({action:'read',id:server.id,count:100}))).length,0);
  await complete(start({action:'write',id:server.id},encoder.encode('reply')));
  host.netSocketControl(server.id,JSON.stringify({action:'shutdown_write'}));
  assert.equal(decoder.decode(await complete(start({action:'read',id:client.id,count:100}))),'reply');
  assert.equal((await complete(start({action:'read',id:client.id,count:100}))).length,0);
  const pending=start({action:'accept',id:listener.id,timeout_ms:0});
  host.netSocketCancel(pending);
  await assert.rejects(complete(pending),/ECANCELLED/);
  await assert.rejects(complete(start({action:'accept',id:listener.id,timeout_ms:5})),/ETIMEOUT/);
  const closing=start({action:'accept',id:listener.id,timeout_ms:0});
  host.netSocketClose(listener.id);
  await assert.rejects(complete(closing),/ECLOSED/);
 } finally {for(const id of [client.id,server.id,listener.id]) host.netSocketClose(id);}
});
test('UDP preserves empty messages, reports truncation, and wakes closed receivers',async()=>{
 const first=await create('udp_bind'),second=await create('udp_bind');
 try {
  await complete(start({action:'udp_send',id:first.id,host:'127.0.0.1',port:second.local_port},encoder.encode('abcdef')));
  const message=JSON.parse(decoder.decode(await complete(start({action:'udp_receive',id:second.id,count:3}))));
  assert.equal(message.bytes,'616263');assert.equal(message.truncated,true);assert.equal(message.port,first.local_port);
  await complete(start({action:'udp_send',id:first.id,host:'127.0.0.1',port:second.local_port}));
  const empty=JSON.parse(decoder.decode(await complete(start({action:'udp_receive',id:second.id,count:3}))));
  assert.equal(empty.bytes,'');assert.equal(empty.truncated,false);
  const pending=start({action:'udp_receive',id:second.id,count:10,timeout_ms:0});
  host.netSocketClose(second.id);
  await assert.rejects(complete(pending),/ECLOSED/);
 } finally {host.netSocketClose(first.id);host.netSocketClose(second.id);}
});
