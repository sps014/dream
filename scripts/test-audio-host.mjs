import test from 'node:test';
import assert from 'node:assert/strict';
import { makeAudioHost } from '../runtime/src/hosts/audio.js';

class FakeNode {
  constructor() { this.gain = { value: 1 }; }
  connect() {}
  disconnect() {}
  start() {}
  stop() {}
}
const contexts = [];
class FakeContext {
  constructor() {
    this.currentTime = 0;
    this.state = 'suspended';
    this.destination = {};
    this.audioWorklet = { addModule: async () => {} };
    contexts.push(this);
  }
  async resume() { this.state = 'running'; }
  async close() { this.state = 'closed'; }
  async setSinkId(id) { if (id === 'denied') throw new Error('permission denied'); }
  createGain() { return new FakeNode(); }
  createBufferSource() { return new FakeNode(); }
  createBuffer(channels, count, sampleRate) {
    const data = Array.from({length: channels}, () => new Float32Array(count));
    return { sampleRate, getChannelData: index => data[index] };
  }
}

test('Node reports unavailable output without initializing a browser context', () => {
  const host = makeAudioHost();
  assert.equal(host.audioOutputCreate([0, 0], 48000, 1), -1);
});

test('players share output, retain positions, and release the final context', async () => {
  globalThis.AudioContext = FakeContext;
  globalThis.AudioWorkletNode = FakeNode;
  try {
    const host = makeAudioHost();
    const samples = new Float32Array(48000);
    const first = host.audioOutputCreate(samples, 48000, 1);
    const second = host.audioOutputCreate(samples, 48000, 1);
    assert.equal(contexts.length, 0);
    assert.deepEqual(await Promise.all([host.audioOutputStart(first), host.audioOutputStart(second)]), [1, 1]);
    assert.equal(contexts.length, 1);
    contexts[0].currentTime = 0.25;
    assert.equal(host.audioOutputQuery(first, 1), 12000);
    assert.equal(host.audioOutputControl(first, 2, 0), 1);
    contexts[0].currentTime = 0.5;
    assert.equal(host.audioOutputQuery(first, 1), 12000);
    assert.equal(host.audioOutputQuery(second, 1), 24000);
    assert.equal(await host.audioOutputSink(first, 'denied'), 0);
    assert.equal(host.audioOutputQuery(first, 1), 12000);
    assert.equal(contexts[0].state, 'running');
    assert.equal(contexts[1].state, 'closed');
    host.audioOutputClose(first);
    assert.equal(contexts[0].state, 'running');
    host.audioOutputClose(second);
    assert.equal(contexts[0].state, 'closed');
    host.audioOutputClose(second);
    assert.equal(await host.audioOutputStart(second), 0);
  } finally {
    delete globalThis.AudioContext;
    delete globalThis.AudioWorkletNode;
  }
});

test('pause and close cancel pending context initialization', async () => {
  let ready;
  const gate = new Promise(resolve => { ready = resolve; });
  globalThis.AudioContext = class extends FakeContext {
    constructor() {
      super();
      this.audioWorklet.addModule = () => gate;
    }
  };
  globalThis.AudioWorkletNode = FakeNode;
  try {
    const host = makeAudioHost();
    const first = host.audioOutputCreate(new Float32Array(48000), 48000, 1);
    const second = host.audioOutputCreate(new Float32Array(48000), 48000, 1);
    const starting = host.audioOutputStart(first);
    const closing = host.audioOutputStart(second);
    host.audioOutputControl(first, 2, 0);
    host.audioOutputClose(second);
    ready();
    assert.deepEqual(await Promise.all([starting, closing]), [-6, -6]);
    assert.equal(host.audioOutputQuery(first, 0), 0);
    assert.equal(await host.audioOutputStart(first), 1);
    host.audioOutputClose(first);
  } finally {
    delete globalThis.AudioContext;
    delete globalThis.AudioWorkletNode;
  }
});

test('browser picker distinguishes activation, denial and unavailable support', async () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
  try {
    const navigator = { userActivation: { isActive: false }, mediaDevices: {
      selectAudioOutput: async () => { throw new DOMException('denied', 'NotAllowedError'); },
    } };
    Object.defineProperty(globalThis, 'navigator', { value: navigator, configurable: true });
    const host = makeAudioHost();
    assert.equal(JSON.parse(await host.audioOutputRequest()).error.code, 'EACTIVATION');
    navigator.userActivation.isActive = true;
    assert.equal(JSON.parse(await host.audioOutputRequest()).error.code, 'EACCES');
    delete navigator.mediaDevices.selectAudioOutput;
    assert.equal(JSON.parse(await host.audioOutputRequest()).error.code, 'ENOTSUP');
  } finally {
    if (original) Object.defineProperty(globalThis, 'navigator', original);
    else delete globalThis.navigator;
  }
});

test('removing a selected device pauses its players and releases its listener', async () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
  const listeners = new Set();
  let devices = [{ kind: 'audiooutput', deviceId: 'usb', label: 'USB' }];
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { mediaDevices: {
    enumerateDevices: async () => devices,
    addEventListener: (_, listener) => listeners.add(listener),
    removeEventListener: (_, listener) => listeners.delete(listener),
  } } });
  globalThis.AudioContext = FakeContext;
  globalThis.AudioWorkletNode = FakeNode;
  try {
    const host = makeAudioHost();
    const player = host.audioOutputCreate(new Float32Array(48000), 48000, 1);
    assert.equal(await host.audioOutputStart(player), 1);
    assert.equal(await host.audioOutputSink(player, 'usb'), 1);
    assert.equal(listeners.size, 1);
    devices = [];
    await Promise.all([...listeners].map(listener => listener()));
    assert.equal(host.audioOutputQuery(player, 3), 1);
    assert.equal(host.audioOutputQuery(player, 0), 0);
    assert.equal(await host.audioOutputStart(player), -5);
    assert.equal(await host.audioOutputSink(player, ''), 1);
    assert.equal(listeners.size, 0);
    assert.equal(await host.audioOutputStart(player), 1);
    host.audioOutputClose(player);
  } finally {
    delete globalThis.AudioContext;
    delete globalThis.AudioWorkletNode;
    if (original) Object.defineProperty(globalThis, 'navigator', original);
    else delete globalThis.navigator;
  }
});
