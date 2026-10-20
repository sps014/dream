import test from 'node:test';
import assert from 'node:assert/strict';
import { makeAudioHost } from '../runtime/src/hosts/audio.js';
import { makeAudioMediaHost } from '../runtime/src/hosts/audio_media.js';

class Node {
  gain = { value: 1 };
  connect() {}
  disconnect() {}
}
class Context {
  state = 'suspended';
  destination = {};
  createMediaElementSource() { return new Node(); }
  createGain() { return new Node(); }
  async resume() { this.state = 'running'; }
  async close() { this.state = 'closed'; }
  async setSinkId(id) { if (id === 'denied') throw new DOMException('denied', 'NotAllowedError'); }
}
class Media extends EventTarget {
  time = 0;
  duration = 100;
  readyState = 4;
  paused = true;
  ended = false;
  seekable = { length: 1, start: () => 0, end: () => 100 };
  get currentTime() { return this.time; }
  set currentTime(value) {
    this.time = value;
    queueMicrotask(() => this.dispatchEvent(new Event('seeked')));
  }
  load() { this.onloadedmetadata?.(); }
  removeAttribute() {}
  async play() { this.paused = false; }
  pause() { this.paused = true; }
}

test('browser URLs delegate loading, seeking and output routing to platform media', async () => {
  const originalDocument = globalThis.document;
  const originalContext = globalThis.AudioContext;
  const elements = [];
  globalThis.document = { createElement: () => { const media = new Media(); elements.push(media); return media; } };
  globalThis.AudioContext = Context;
  try {
    const host = { ...makeAudioHost(), ...makeAudioMediaHost() };
    const player = host.audioMediaCreate('https://audio.example/music.mp3');
    assert.equal(elements[0].src, 'https://audio.example/music.mp3');
    assert.equal(elements[0].crossOrigin, 'anonymous');
    assert.equal(host.audioOutputQuery(player, 7), 0);
    elements[0].oncanplay();
    assert.equal(host.audioOutputQuery(player, 7), 1);
    assert.equal(await host.audioOutputStart(player), 1);
    assert.equal(await host.audioMediaSeek(player, 75), 1);
    assert.equal(host.audioOutputQuery(player, 1), 75 * 48000);
    assert.equal(await host.audioMediaSeek(player, 101), -1);
    assert.equal(host.audioOutputQuery(player, 1), 75 * 48000);
    assert.equal(await host.audioOutputSink(player, 'denied'), -2);
    assert.equal(host.audioOutputQuery(player, 1), 75 * 48000);
    host.audioOutputControl(player, 2, 0);
    assert.equal(elements[0].paused, true);
    elements[0].onerror();
    assert.equal(host.audioOutputQuery(player, 7), -1);
    host.audioOutputClose(player);
    assert.equal(await host.audioMediaSeek(player, 10), -6);
  } finally {
    if (originalDocument === undefined) delete globalThis.document;
    else globalThis.document = originalDocument;
    if (originalContext === undefined) delete globalThis.AudioContext;
    else globalThis.AudioContext = originalContext;
  }
});
