import test from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as crypto from 'node:crypto';
import {setNodeFs,setNodeCrypto} from '../runtime/src/platform.js';
import {makeFsHost} from '../runtime/src/hosts/fs.js';
import {makeCryptoHost} from '../runtime/src/hosts/crypto.js';
setNodeFs(fs);setNodeCrypto(crypto);
test('temporary resources use private unique directories and atomic replacement',()=>{
 const host=makeFsHost();
 assert.equal(host.tempSupported(),true);
 const one=host.tempDirectoryCreate(''),two=host.tempDirectoryCreate('');
 try {
  assert.ok(one && two && one!==two);
  if(process.platform!=='win32') assert.equal(fs.statSync(one).mode&0o777,0o700);
  fs.writeFileSync(one+'/input','new');fs.writeFileSync(one+'/target','old');
  assert.equal(host.tempReplace(one+'/input',one+'/target'),true);
  assert.equal(fs.readFileSync(one+'/target','utf8'),'new');
  fs.writeFileSync(one+'/input','retained');
  assert.equal(host.tempReplace(one+'/input',one+'/missing/target'),false);
  assert.equal(fs.readFileSync(one+'/input','utf8'),'retained');
 } finally {fs.rmSync(one,{recursive:true,force:true});fs.rmSync(two,{recursive:true,force:true});}
});
test('secure random adapter exposes entropy failure instead of zero-filled output',()=>{
 const descriptor=Object.getOwnPropertyDescriptor(globalThis,'crypto');
 setNodeCrypto(null);
 try {
  Object.defineProperty(globalThis,'crypto',{configurable:true,value:{getRandomValues(){throw new Error('injected entropy failure');}}});
  assert.deepEqual(makeCryptoHost().cryptoSecureRandomBytes(16),[]);
  assert.deepEqual(makeCryptoHost().cryptoSecureRandomBytes(-1),[]);
 } finally {if(descriptor) Object.defineProperty(globalThis,'crypto',descriptor);else delete globalThis.crypto;setNodeCrypto(crypto);}
});
