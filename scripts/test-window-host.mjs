import test from 'node:test';
import assert from 'node:assert/strict';
import { makeWindowHost } from '../runtime/src/hosts/window.js';

// A minimal DOM: canvases are EventTargets, so the host's listeners receive dispatched events.
class FakeCanvas extends EventTarget {
  constructor(id = '') {
    super();
    this.id = id;
    this.tagName = 'CANVAS';
    this.style = {};
    this.width = 300;
    this.height = 150;
    this.tabIndex = -1;
    this.cssWidth = 400;
    this.cssHeight = 200;
    this.removed = false;
  }
  getBoundingClientRect() { return { left: 10, top: 20, width: this.cssWidth, height: this.cssHeight }; }
  focus() {
    if (globalThis.document.activeElement === this) return;
    globalThis.document.activeElement = this;
    this.dispatchEvent(new Event('focus'));
  }
  requestPointerLock() { globalThis.document.pointerLockElement = this; }
  remove() { this.removed = true; }
}

function installDom({ ratio = 2, pads = [] } = {}) {
  const page = new FakeCanvas('game');
  const body = { children: [], appendChild(child) { this.children.push(child); } };
  globalThis.document = {
    title: '', body, activeElement: null, pointerLockElement: null, fullscreenElement: null,
    createElement: tag => (tag === 'canvas' ? new FakeCanvas() : { tagName: tag.toUpperCase() }),
    getElementById: id => (id === 'game' ? page : id === 'label' ? { tagName: 'DIV' } : null),
    exitPointerLock() { this.pointerLockElement = null; },
  };
  globalThis.devicePixelRatio = ratio;
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { getGamepads: () => pads } });
  return page;
}

function uninstallDom() {
  delete globalThis.document;
  delete globalThis.devicePixelRatio;
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: undefined });
}

function fire(target, type, props = {}) {
  const event = Object.assign(new Event(type, { cancelable: true }), props);
  target.dispatchEvent(event);
  return event;
}

function records(values) {
  const out = [];
  for (let at = 0; at + 1 < values.length;) {
    const length = values[at + 1];
    out.push([values[at], ...values.slice(at + 2, at + 2 + length)]);
    at += 2 + length;
  }
  return out;
}

test('without a DOM, windows fail with ENODISPLAY instead of throwing', () => {
  uninstallDom();
  const host = makeWindowHost();
  assert.equal(host.windowOpen('t', 10, 10, 0), 0);
  assert.match(host.windowError(), /^ENODISPLAY\n/);
  assert.equal(host.windowCanvas('game', 5), 0);
  assert.deepEqual(host.windowPoll(1), []);
  assert.deepEqual(host.windowQuery(1), []);
  assert.equal(host.windowGamepadRumble(0, 1, 1, 100), false);
});

test('canvas lookup reports missing and non-canvas elements', () => {
  installDom();
  try {
    const host = makeWindowHost();
    assert.equal(host.windowCanvas('missing', 5), 0);
    assert.match(host.windowError(), /^ENOTFOUND\n/);
    assert.equal(host.windowCanvas('label', 5), 0);
  } finally { uninstallDom(); }
});

test('keyboard events carry physical codes, layout keys, modifiers and text', () => {
  const page = installDom();
  try {
    const host = makeWindowHost();
    const id = host.windowCanvas('game', 5);
    assert.ok(id > 0);
    assert.equal(page.tabIndex, 0, 'canvas becomes focusable');
    const down = fire(page, 'keydown', { code: 'KeyQ', key: 'a', shiftKey: true });
    assert.equal(down.defaultPrevented, true);
    fire(page, 'keydown', { code: 'KeyQ', key: 'a', repeat: true });
    fire(page, 'keyup', { code: 'KeyQ', key: 'a' });
    const shortcut = fire(page, 'keydown', { code: 'KeyC', key: 'c', metaKey: true });
    assert.equal(shortcut.defaultPrevented, false, 'browser shortcuts keep working');
    fire(page, 'keydown', { code: 'Escape', key: 'Escape' });
    fire(page, 'keydown', { code: 'F11', key: 'F11' });
    fire(page, 'compositionend', { data: '日本' });
    assert.deepEqual(records(host.windowPoll(id)), [
      [6, 17, 1, 0, 97], [8, 97],
      [6, 17, 0, 1, 97], [8, 97],
      [7, 17, 0, 0, 97],
      [6, 3, 8, 0, 99],
      [6, 61, 0, 0],
      [6, 47, 0, 0],
      [8, 0x65e5, 0x672c],
    ]);
    assert.deepEqual(host.windowPoll(id), [], 'a poll drains the queue');
  } finally { uninstallDom(); }
});

test('pointer, wheel and Pointer Lock use logical coordinates', () => {
  const page = installDom();
  try {
    const host = makeWindowHost();
    const id = host.windowCanvas('game', 5);
    fire(page, 'pointermove', { clientX: 110, clientY: 70 });
    fire(page, 'pointerdown', { button: 2 });
    fire(page, 'pointerup', { button: 2 });
    fire(page, 'pointerdown', { button: 9 });
    const wheel = fire(page, 'wheel', { deltaX: 0, deltaY: 200, deltaMode: 0 });
    assert.equal(wheel.defaultPrevented, true);
    fire(page, 'wheel', { deltaX: 3, deltaY: 0, deltaMode: 1 });
    host.windowSetCursorMode(id, 2);
    assert.equal(document.pointerLockElement, page);
    assert.equal(page.style.cursor, 'none');
    fire(page, 'pointermove', { clientX: 0, clientY: 0, movementX: 5, movementY: -4 });
    host.windowSetCursorMode(id, 0);
    assert.equal(document.pointerLockElement, null);
    host.windowSetCursorIcon(id, 3);
    assert.equal(page.style.cursor, 'pointer');
    assert.deepEqual(records(host.windowPoll(id)), [
      [9, 100, 50], [3], [10, 1], [11, 1], [12, 0, -2], [12, 1, 0], [9, 105, 46],
    ]);
  } finally { uninstallDom(); }
});

test('blur releases held input before reporting Unfocused', () => {
  const page = installDom();
  try {
    const host = makeWindowHost();
    const id = host.windowCanvas('game', 5);
    page.focus();
    fire(page, 'keydown', { code: 'KeyW', key: 'w' });
    fire(page, 'pointerdown', { button: 0 });
    host.windowPoll(id);
    fire(page, 'blur');
    assert.deepEqual(records(host.windowPoll(id)), [[7, 23, 0, 0], [11, 0], [4]]);
    assert.deepEqual(host.windowQuery(id).slice(4), [2, 0, 1]);
  } finally { uninstallDom(); }
});

test('resizes follow the canvas layout size times devicePixelRatio', () => {
  const page = installDom({ ratio: 2 });
  try {
    const host = makeWindowHost();
    const id = host.windowCanvas('game', 5);
    assert.deepEqual([page.width, page.height], [800, 400], 'drawable size starts at layout x ratio');
    assert.deepEqual(host.windowPoll(id), [], 'no Resized for the initial size');
    page.cssWidth = 500;
    assert.deepEqual(records(host.windowPoll(id)), [[1, 1000, 400]]);
    assert.deepEqual(host.windowQuery(id).slice(0, 4), [500, 200, 1000, 400]);
    globalThis.devicePixelRatio = 1;
    assert.deepEqual(records(host.windowPoll(id)), [[1, 500, 200]]);
    host.windowSetSize(id, 640, 360);
    assert.equal(page.style.width, '640px');
  } finally { uninstallDom(); }
});

test('gamepads connect, sample in standard order and disconnect', () => {
  const pressed = index => ({ pressed: true, value: 1 });
  const idle = { pressed: false, value: 0 };
  const buttons = Array.from({ length: 17 }, () => idle);
  buttons[0] = pressed(0);
  buttons[15] = pressed(15);
  buttons[7] = { pressed: false, value: 0.5 };
  const pads = [null, { index: 1, id: 'Pad', connected: true, axes: [0.25, -1, 0, 2], buttons }];
  const page = installDom({ pads });
  try {
    const host = makeWindowHost();
    const id = host.windowCanvas('game', 5);
    assert.deepEqual(records(host.windowPoll(id)), [
      [13, 1, 80, 97, 100],
      [16, 1, (1 << 0) | (1 << 12), 0.25, -1, 0, 1, 0, 0.5],
    ]);
    pads.length = 0;
    assert.deepEqual(records(host.windowPoll(id)), [[14, 1]]);
  } finally { uninstallDom(); }
});

test('open creates and close removes an owned canvas', () => {
  installDom();
  try {
    const host = makeWindowHost();
    assert.equal(host.windowOpen('t', 0, 10, 0), 0);
    assert.match(host.windowError(), /^EINVAL\n/);
    const id = host.windowOpen('Game', 320, 240, 1);
    const canvas = document.body.children[0];
    assert.equal(document.title, 'Game');
    assert.equal(canvas.style.width, '320px');
    host.windowSetTitle(id, 'Renamed');
    assert.equal(document.title, 'Renamed');
    host.windowClose(id);
    assert.equal(canvas.removed, true);
    fire(canvas, 'keydown', { code: 'KeyA', key: 'a' });
    assert.deepEqual(host.windowPoll(id), []);
  } finally { uninstallDom(); }
});
