// Browser windows for `system.window`: a canvas plus keyboard, pointer, wheel, Pointer Lock and
// Gamepad API input. `windowPoll` returns the same `[kind, length, payload...]` float records
// as the native GLFW host (crates/dream-host-window/src/wire.rs), decoded by
// crates/dream-stdlib/src/system/window/input_wire.dream.

const RESIZED = 1;
const FOCUSED = 3;
const UNFOCUSED = 4;
const KEY_DOWN = 6;
const KEY_UP = 7;
const TEXT = 8;
const MOUSE_MOVE = 9;
const MOUSE_DOWN = 10;
const MOUSE_UP = 11;
const SCROLL = 12;
const GAMEPAD_CONNECTED = 13;
const GAMEPAD_DISCONNECTED = 14;
const FILE_DROPPED = 15;
const GAMEPAD_SAMPLE = 16;

const FLAG_FULLSCREEN = 2;
const CURSOR_CAPTURED = 2;
const GAMEPAD_SLOTS = 16;
const CURSOR_ICONS = ["default", "text", "crosshair", "pointer", "ew-resize", "ns-resize",
  "nwse-resize", "nesw-resize", "move", "not-allowed"];
// `KeyCode` values from 61 on, in enum order; KeyboardEvent.code uses the same names.
const NAMED_KEYS = ["Escape", "Enter", "Tab", "Backspace", "Space", "Insert", "Delete",
  "ArrowRight", "ArrowLeft", "ArrowDown", "ArrowUp", "PageUp", "PageDown", "Home", "End",
  "CapsLock", "ScrollLock", "NumLock", "PrintScreen", "Pause", "Minus", "Equal", "BracketLeft",
  "BracketRight", "Backslash", "Semicolon", "Quote", "Backquote", "Comma", "Period", "Slash",
  "IntlBackslash", "Numpad0", "Numpad1", "Numpad2", "Numpad3", "Numpad4", "Numpad5", "Numpad6",
  "Numpad7", "Numpad8", "Numpad9", "NumpadDecimal", "NumpadDivide", "NumpadMultiply",
  "NumpadSubtract", "NumpadAdd", "NumpadEnter", "NumpadEqual", "ShiftLeft", "ControlLeft",
  "AltLeft", "MetaLeft", "ShiftRight", "ControlRight", "AltRight", "MetaRight", "ContextMenu"];
// Browser `MouseEvent.button` (0 left, 1 middle, 2 right) to `MouseButton` order.
const MOUSE_BUTTONS = [0, 2, 1, 3, 4];
// Standard Gamepad mapping button index for each `GamepadButton`, in Dream order.
const PAD_BUTTONS = [0, 1, 2, 3, 4, 5, 8, 9, 16, 10, 11, 12, 15, 13, 14];

const KEY_CODES = (() => {
  const codes = Object.create(null);
  for (let i = 0; i < 26; i++) codes["Key" + String.fromCharCode(65 + i)] = 1 + i;
  for (let i = 0; i < 10; i++) codes["Digit" + i] = 27 + i;
  for (let i = 1; i <= 24; i++) codes["F" + i] = 36 + i;
  NAMED_KEYS.forEach((name, i) => { codes[name] = 61 + i; });
  return codes;
})();

function utf16(text) {
  const units = [];
  for (let i = 0; i < text.length; i++) units.push(text.charCodeAt(i));
  return units;
}

function push(state, kind, payload, text = "") {
  const units = utf16(text);
  state.queue.push(kind, payload.length + units.length, ...payload, ...units);
}

function modifiers(e) {
  const lock = name => (typeof e.getModifierState === "function" && e.getModifierState(name));
  return (e.shiftKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.altKey ? 4 : 0) | (e.metaKey ? 8 : 0)
    | (lock("CapsLock") ? 16 : 0) | (lock("NumLock") ? 32 : 0);
}

// `KeyboardEvent.key` is a name such as "ArrowUp" for keys that print nothing.
function printable(key) {
  return typeof key === "string" && [...key].length === 1 ? key : "";
}

function pixelRatio() {
  return (typeof globalThis.devicePixelRatio === "number" && globalThis.devicePixelRatio > 0)
    ? globalThis.devicePixelRatio : 1;
}

function cssSize(canvas) {
  const rect = canvas.getBoundingClientRect();
  return [rect.width || canvas.clientWidth || canvas.width, rect.height || canvas.clientHeight || canvas.height];
}

function locked(state) {
  return globalThis.document?.pointerLockElement === state.canvas;
}

function requestLock(state) {
  try {
    const request = state.canvas.requestPointerLock?.();
    request?.catch?.(() => {});
  } catch (_) {
    // Browsers refuse the lock outside a user gesture; the next pointerdown retries.
  }
}

function releaseHeld(state) {
  for (const code of state.keys) push(state, KEY_UP, [code, 0, 0]);
  for (const button of state.buttons) push(state, MOUSE_UP, [button]);
  state.keys.clear();
  state.buttons.clear();
}

function pointerPosition(state, e) {
  if (locked(state)) {
    state.x += e.movementX || 0;
    state.y += e.movementY || 0;
  } else {
    const rect = state.canvas.getBoundingClientRect();
    state.x = e.clientX - rect.left;
    state.y = e.clientY - rect.top;
  }
  push(state, MOUSE_MOVE, [state.x, state.y]);
}

function applyCursor(state) {
  state.canvas.style.cursor = state.cursorMode === 0 ? CURSOR_ICONS[state.icon] : "none";
}

function syncSize(state, report) {
  const [width, height] = cssSize(state.canvas);
  const ratio = pixelRatio();
  const pixels = [Math.max(1, Math.round(width * ratio)), Math.max(1, Math.round(height * ratio))];
  if (pixels[0] !== state.pixels[0] || pixels[1] !== state.pixels[1]) {
    state.pixels = pixels;
    state.canvas.width = pixels[0];
    state.canvas.height = pixels[1];
    if (report) push(state, RESIZED, pixels);
  }
}

function pollGamepads(state) {
  const pads = typeof globalThis.navigator?.getGamepads === "function" ? globalThis.navigator.getGamepads() : [];
  const present = new Set();
  for (const pad of pads || []) {
    if (!pad || pad.connected === false || pad.index >= GAMEPAD_SLOTS) continue;
    present.add(pad.index);
    if (!state.pads.has(pad.index)) {
      state.pads.add(pad.index);
      push(state, GAMEPAD_CONNECTED, [pad.index], String(pad.id || ""));
    }
    let buttons = 0;
    PAD_BUTTONS.forEach((index, bit) => {
      if (pad.buttons?.[index]?.pressed) buttons |= 1 << bit;
    });
    const axis = i => Math.max(-1, Math.min(1, Number(pad.axes?.[i]) || 0));
    const trigger = i => Math.max(0, Math.min(1, Number(pad.buttons?.[i]?.value) || 0));
    push(state, GAMEPAD_SAMPLE, [pad.index, buttons, axis(0), axis(1), axis(2), axis(3), trigger(6), trigger(7)]);
  }
  for (const slot of [...state.pads]) {
    if (!present.has(slot)) {
      state.pads.delete(slot);
      push(state, GAMEPAD_DISCONNECTED, [slot]);
    }
  }
}

function attach(canvas, owns) {
  const state = {
    canvas, owns, queue: [], keys: new Set(), buttons: new Set(), pads: new Set(),
    listeners: [], x: 0, y: 0, cursorMode: 0, icon: 0, pixels: [0, 0],
  };
  const listen = (target, type, handler, options) => {
    target.addEventListener(type, handler, options);
    state.listeners.push([target, type, handler, options]);
  };
  if (!(canvas.tabIndex >= 0)) canvas.tabIndex = 0;
  syncSize(state, false);
  listen(canvas, "keydown", e => {
    const code = KEY_CODES[e.code] || 0;
    const key = printable(e.key);
    push(state, KEY_DOWN, [code, modifiers(e), e.repeat ? 1 : 0], key);
    if (code) state.keys.add(code);
    if (key && !e.ctrlKey && !e.metaKey && !e.isComposing) push(state, TEXT, [], key);
    // Keep browser shortcuts working while games get Tab, Space and the arrows.
    if (!e.ctrlKey && !e.metaKey && !/^F\d+$/.test(e.code || "")) e.preventDefault?.();
  });
  listen(canvas, "keyup", e => {
    const code = KEY_CODES[e.code] || 0;
    push(state, KEY_UP, [code, modifiers(e), 0], printable(e.key));
    state.keys.delete(code);
  });
  listen(canvas, "compositionend", e => {
    if (e.data) push(state, TEXT, [], e.data);
  });
  listen(canvas, "pointermove", e => pointerPosition(state, e));
  listen(canvas, "pointerdown", e => {
    canvas.focus?.();
    if (state.cursorMode === CURSOR_CAPTURED && !locked(state)) requestLock(state);
    const button = MOUSE_BUTTONS[e.button];
    if (button === undefined) return;
    state.buttons.add(button);
    push(state, MOUSE_DOWN, [button]);
  });
  listen(canvas, "pointerup", e => {
    const button = MOUSE_BUTTONS[e.button];
    if (button === undefined) return;
    state.buttons.delete(button);
    push(state, MOUSE_UP, [button]);
  });
  listen(canvas, "contextmenu", e => e.preventDefault?.());
  listen(canvas, "wheel", e => {
    e.preventDefault?.();
    // One notch is 1.0: about 100 pixels or 3 lines in pixel and line delta modes.
    const unit = e.deltaMode === 1 ? 3 : e.deltaMode === 2 ? 1 : 100;
    push(state, SCROLL, [(e.deltaX || 0) / unit, (0 - (e.deltaY || 0)) / unit]);
  }, { passive: false });
  listen(canvas, "focus", () => push(state, FOCUSED, []));
  listen(canvas, "blur", () => {
    releaseHeld(state);
    push(state, UNFOCUSED, []);
  });
  listen(canvas, "dragover", e => e.preventDefault?.());
  listen(canvas, "drop", e => {
    e.preventDefault?.();
    for (const file of e.dataTransfer?.files || []) push(state, FILE_DROPPED, [], String(file.name));
  });
  return state;
}

// Canvases by window handle for capability hosts that draw into windows (gpu.js). Handles are
// unique across every window host on the page so this lookup cannot mix up two instances.
const surfaceCanvases = new Map();
let nextWindowId = 1;

/** The canvas behind a live window handle, or null. */
export function windowSurfaceCanvas(handle) {
  return surfaceCanvases.get(handle) || null;
}

export function makeWindowHost() {
  const windows = new Map();
  let lastError = "";
  const fail = (code, message) => {
    lastError = code + "\n" + message;
    return 0;
  };
  const register = state => {
    const id = nextWindowId++;
    windows.set(id, state);
    surfaceCanvases.set(id, state.canvas);
    return id;
  };
  const noDisplay = () => fail("ENODISPLAY", "no display is available: this JavaScript host has no DOM");

  return {
    windowOpen: (title, width, height, flags) => {
      const doc = globalThis.document;
      if (!doc?.createElement || !doc.body) return noDisplay();
      if (!(width > 0 && height > 0)) return fail("EINVAL", "window width and height must be positive");
      const canvas = doc.createElement("canvas");
      canvas.style.width = width + "px";
      canvas.style.height = height + "px";
      doc.body.appendChild(canvas);
      doc.title = title;
      const state = attach(canvas, true);
      if (flags & FLAG_FULLSCREEN) canvas.requestFullscreen?.()?.catch?.(() => {});
      return register(state);
    },
    windowCanvas: (id, flags) => {
      const doc = globalThis.document;
      if (!doc?.getElementById) return noDisplay();
      const canvas = doc.getElementById(id);
      if (!canvas || String(canvas.tagName).toLowerCase() !== "canvas") {
        return fail("ENOTFOUND", "no <canvas> element has the id '" + id + "'");
      }
      const state = attach(canvas, false);
      if (flags & FLAG_FULLSCREEN) canvas.requestFullscreen?.()?.catch?.(() => {});
      return register(state);
    },
    windowError: () => lastError || "EWINDOW\nwindow creation failed",
    windowClose: handle => {
      const state = windows.get(handle);
      if (!state) return;
      windows.delete(handle);
      surfaceCanvases.delete(handle);
      for (const [target, type, handler, options] of state.listeners) {
        target.removeEventListener(type, handler, options);
      }
      if (locked(state)) globalThis.document.exitPointerLock?.();
      if (state.owns) state.canvas.remove?.();
    },
    windowPoll: handle => {
      const state = windows.get(handle);
      if (!state) return [];
      syncSize(state, true);
      pollGamepads(state);
      const records = state.queue;
      state.queue = [];
      return records;
    },
    windowQuery: handle => {
      const state = windows.get(handle);
      if (!state) return [];
      const [width, height] = cssSize(state.canvas);
      const doc = globalThis.document;
      return [width, height, state.pixels[0], state.pixels[1], pixelRatio(),
        doc?.fullscreenElement === state.canvas ? 1 : 0, doc?.activeElement === state.canvas ? 1 : 0];
    },
    windowSetTitle: (handle, title) => {
      const state = windows.get(handle);
      if (state?.owns) globalThis.document.title = title;
    },
    windowSetSize: (handle, width, height) => {
      const state = windows.get(handle);
      if (!state || !(width > 0 && height > 0)) return;
      state.canvas.style.width = width + "px";
      state.canvas.style.height = height + "px";
    },
    windowSetFullscreen: (handle, enabled) => {
      const state = windows.get(handle);
      if (!state) return;
      const doc = globalThis.document;
      if (enabled) state.canvas.requestFullscreen?.()?.catch?.(() => {});
      else if (doc?.fullscreenElement === state.canvas) doc.exitFullscreen?.()?.catch?.(() => {});
    },
    windowSetCursorMode: (handle, mode) => {
      const state = windows.get(handle);
      if (!state) return;
      state.cursorMode = mode;
      applyCursor(state);
      if (mode === CURSOR_CAPTURED) requestLock(state);
      else if (locked(state)) globalThis.document.exitPointerLock?.();
    },
    windowSetCursorIcon: (handle, icon) => {
      const state = windows.get(handle);
      if (!state) return;
      state.icon = icon >= 0 && icon < CURSOR_ICONS.length ? icon : 0;
      applyCursor(state);
    },
    windowGamepadRumble: (slot, low, high, ms) => {
      const pads = typeof globalThis.navigator?.getGamepads === "function" ? globalThis.navigator.getGamepads() : [];
      const actuator = pads?.[slot]?.vibrationActuator;
      if (typeof actuator?.playEffect !== "function") return false;
      actuator.playEffect("dual-rumble", {
        duration: ms,
        strongMagnitude: Math.max(0, Math.min(1, low)),
        weakMagnitude: Math.max(0, Math.min(1, high)),
      })?.catch?.(() => {});
      return true;
    },
  };
}
