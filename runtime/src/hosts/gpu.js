/**
 * WebGPU host for `system.gpu`. Buffers/textures/surfaces/samplers are tracked by integer id;
 * kernels come from the sibling `.wgsl` + `abi.gpu.kernels` metadata attached via `attachGpuAbi`.
 */
import { replaceArtifactExt } from "../urls.js";

function makeGpuHost(getInstance) {
  const buffers = new Map(); // id -> { gpuBuffer, nbytes, cpu, usage }
  const shaders = new Map();
  const textures = new Map(); // id -> { texture, width, height, cpu, storage }
  const samplers = new Map(); // id -> { sampler, filter }
  const surfaces = new Map();
  const querySets = new Map();
  const passes = new Map(); // id -> { ops, querySet, tsBegin, tsEnd }
  const pipelineCache = new Map();
  const renderPipelines = new Map(); // id -> { pipeline, vsMeta, fsMeta, layouts, groups }
  const bindGroups = new Map(); // id -> { pipelineId, group, bufferIds, textureIds, samplerIds }
  const renderBgCache = new Map(); // `pipeline:group:handle` -> GPUBindGroup
  const renderPipelineCache = new Map(); // key -> id
  let nextId = 1;
  let devicePromise = null;
  let device = null;
  // Kept alongside the device because subgroup width is adapter information, not a device limit.
  let gpuAdapter = null;
  let gpuAbi = null;
  let wgslSource = null;
  let blitPipeline = null;
  let blitSampler = null;
  let blitBindLayout = null;

  let lastError = "";
  let deviceLost = false;
  let powerCode = 2;

  const ERR_UNAVAILABLE = 1;
  const ERR_TIMEOUT = 2;
  const ERR_VALIDATION = 3;
  const ERR_OTHER = 4;
  const ERR_UNSUPPORTED = 5;
  const ERR_DEVICE_LOST = 6;

  // Optional WebGPU features Dream opts into whenever the adapter offers them. A shader or resource
  // may only touch a feature the *device* asked for — reaching for an un-requested one is
  // device-loss-grade rather than a recoverable validation error — so this list is a contract with
  // `gpuCapabilities`. Mirrors `wanted_features` in native `src/execution/host/gpu/caps.rs`.
  const WANTED_FEATURES = [
    "shader-f16",
    "subgroups",
    "texture-compression-bc",
    "texture-compression-etc2",
    "texture-compression-astc",
    "depth32float-stencil8",
    "float32-filterable",
    "timestamp-query",
    "timestamp-query-inside-encoders",
  ];

  // Limits raised to the adapter's own maximum. Everything else stays at the portable WebGPU
  // default so a program developed against a large GPU still runs on a small one.
  const RAISED_LIMITS = [
    "maxBufferSize",
    "maxStorageBufferBindingSize",
    "maxComputeWorkgroupStorageSize",
    "maxComputeInvocationsPerWorkgroup",
    "maxComputeWorkgroupSizeX",
    "maxComputeWorkgroupSizeY",
    "maxComputeWorkgroupSizeZ",
    "maxComputeWorkgroupsPerDimension",
  ];

  // Packed `gpuCapabilities` blob; see `caps.rs` for the authoritative field offsets.
  const CAPS_BLOB_LEN = 56;
  const CAP_SHADER_FLOAT16 = 1 << 0;
  const CAP_SUBGROUP = 1 << 1;
  const CAP_TEXTURE_COMPRESSION_BC = 1 << 5;
  const CAP_TEXTURE_COMPRESSION_ETC2 = 1 << 6;
  const CAP_TEXTURE_COMPRESSION_ASTC = 1 << 7;
  const CAP_DEPTH32_FLOAT_STENCIL8 = 1 << 8;
  const CAP_FLOAT32_FILTERABLE = 1 << 9;
  const CAP_TIMESTAMP_QUERY = 1 << 10;
  const CAP_TIMESTAMP_QUERY_INSIDE_ENCODERS = 1 << 11;

  // Mirrors of the enum tables owned by `crates/dream-abi/src/gpu_format.rs`, indexed by the
  // `GpuTextureFormat` / `GpuTextureDimension` / … discriminants Dream passes as ints. Keep these
  // in step with that file: the codes are the wire format, so reordering silently reinterprets
  // every guest call.
  const TEXTURE_FORMATS = [
    { name: "r8unorm", bytesPerTexel: 1, storage: false },
    { name: "rg8unorm", bytesPerTexel: 2, storage: false },
    { name: "rgba8unorm", bytesPerTexel: 4, storage: true },
    { name: "rgba8unorm-srgb", bytesPerTexel: 4, storage: false },
    { name: "bgra8unorm", bytesPerTexel: 4, storage: false },
    { name: "bgra8unorm-srgb", bytesPerTexel: 4, storage: false },
    { name: "r16float", bytesPerTexel: 2, storage: true },
    { name: "rg16float", bytesPerTexel: 4, storage: true },
    { name: "rgba16float", bytesPerTexel: 8, storage: true },
    { name: "r32float", bytesPerTexel: 4, storage: true, feature: "float32-filterable" },
    { name: "rg32float", bytesPerTexel: 8, storage: true, feature: "float32-filterable" },
    { name: "rgba32float", bytesPerTexel: 16, storage: true, feature: "float32-filterable" },
    { name: "rg11b10ufloat", bytesPerTexel: 4, storage: false },
    { name: "rgb10a2unorm", bytesPerTexel: 4, storage: false },
    { name: "depth16unorm", bytesPerTexel: 0, storage: false, depth: true },
    { name: "depth24plus", bytesPerTexel: 0, storage: false, depth: true },
    { name: "depth24plus-stencil8", bytesPerTexel: 0, storage: false, depth: true },
    { name: "depth32float", bytesPerTexel: 0, storage: false, depth: true },
    {
      name: "depth32float-stencil8",
      bytesPerTexel: 0,
      storage: false,
      depth: true,
      feature: "depth32float-stencil8",
    },
    { name: "bc1-rgba-unorm", bytesPerTexel: 0, storage: false, block: 4, feature: "texture-compression-bc" },
    { name: "bc3-rgba-unorm", bytesPerTexel: 0, storage: false, block: 4, feature: "texture-compression-bc" },
    { name: "bc5-rg-unorm", bytesPerTexel: 0, storage: false, block: 4, feature: "texture-compression-bc" },
    { name: "bc7-rgba-unorm", bytesPerTexel: 0, storage: false, block: 4, feature: "texture-compression-bc" },
    { name: "etc2-rgb8unorm", bytesPerTexel: 0, storage: false, block: 4, feature: "texture-compression-etc2" },
    { name: "etc2-rgba8unorm", bytesPerTexel: 0, storage: false, block: 4, feature: "texture-compression-etc2" },
    { name: "astc-4x4-unorm", bytesPerTexel: 0, storage: false, block: 4, feature: "texture-compression-astc" },
    { name: "astc-8x8-unorm", bytesPerTexel: 0, storage: false, block: 8, feature: "texture-compression-astc" },
  ];
  const TEXTURE_DIMENSIONS = ["1d", "2d", "3d"];
  const VIEW_DIMENSIONS = ["1d", "2d", "2d-array", "cube", "cube-array", "3d"];
  const FILTER_MODES = ["nearest", "linear"];
  const ADDRESS_MODES = ["clamp-to-edge", "repeat", "mirror-repeat"];
  const COMPARE_FUNCTIONS = [
    "never", "less", "equal", "less-equal", "greater", "not-equal", "greater-equal", "always",
  ];
  const VERTEX_FORMATS = [
    "uint8x2", "uint8x4", "sint8x2", "sint8x4",
    "unorm8x2", "unorm8x4", "snorm8x2", "snorm8x4",
    "uint16x2", "uint16x4", "sint16x2", "sint16x4",
    "unorm16x2", "unorm16x4", "snorm16x2", "snorm16x4",
    "float16x2", "float16x4",
    "float32", "float32x2", "float32x3", "float32x4",
    "uint32", "uint32x2", "uint32x3", "uint32x4",
    "sint32", "sint32x2", "sint32x3", "sint32x4",
  ];

  /// WebGPU shape rules, checked up front so a bad descriptor reports a Dream error instead of a
  /// device-level validation message. Returns an error string, or `null` when the shape is legal.
  function validateTextureShape(spec, dim, view, width, height, layers, mips, samples) {
    const cube = view === "cube" || view === "cube-array";
    if (cube) {
      if (layers % 6 !== 0) {
        return `validation: a cube view needs a multiple of 6 layers, got ${layers}`;
      }
      if (width !== height) {
        return `validation: cube faces must be square, got ${width}x${height}`;
      }
    }
    if (view === "3d" && dim !== "3d") return "validation: a 3d view needs a 3d texture";
    if (dim === "3d" && cube) return "validation: a 3d texture cannot have a cube view";
    if (dim === "1d" && height > 1) {
      return `validation: a 1d texture must be 1 texel tall, got height ${height}`;
    }
    if (samples > 1) {
      if (mips > 1) return "validation: a multisampled texture cannot have mip levels";
      if (layers > 1) return "validation: a multisampled texture cannot be layered";
      if (dim !== "2d") return "validation: only 2d textures can be multisampled";
      if (![2, 4, 8, 16].includes(samples)) {
        return `validation: unsupported sample count ${samples}`;
      }
      if (spec.block) return "validation: block-compressed textures cannot be multisampled";
    }
    const extent = Math.max(width, height, dim === "3d" ? layers : 1);
    const maxMips = Math.floor(Math.log2(extent)) + 1;
    if (mips > maxMips) {
      return `validation: ${width}x${height} allows at most ${maxMips} mip levels, got ${mips}`;
    }
    return null;
  }

  function classifyErr(err) {
    const msg = String(err && err.message ? err.message : err);
    lastError = msg;
    if (/device lost|lost device|parent device is lost/i.test(msg)) return ERR_DEVICE_LOST;
    if (/unsupported/i.test(msg)) return ERR_UNSUPPORTED;
    if (/not available|no WebGPU|no WebGPU adapter/i.test(msg)) return ERR_UNAVAILABLE;
    if (/timed out|timeout/i.test(msg)) return ERR_TIMEOUT;
    if (/WGSL|validation|compile/i.test(msg)) return ERR_VALIDATION;
    return ERR_OTHER;
  }

  async function ensureDevice() {
    if (device) return device;
    if (!devicePromise) {
      devicePromise = (async () => {
        if (!globalThis.navigator?.gpu) {
          throw new Error("WebGPU is not available in this environment");
        }
        const adapterOpts = {};
        if ((powerCode | 0) === 1) adapterOpts.powerPreference = "low-power";
        else if ((powerCode | 0) === 2) adapterOpts.powerPreference = "high-performance";
        const adapter = await Promise.race([
          navigator.gpu.requestAdapter(adapterOpts),
          new Promise((_, reject) =>
            setTimeout(() => reject(new Error("WebGPU requestAdapter timed out")), 8000),
          ),
        ]);
        if (!adapter) throw new Error("no WebGPU adapter");
        const requiredFeatures = WANTED_FEATURES.filter((f) => adapter.features?.has(f));
        const requiredLimits = {};
        for (const key of RAISED_LIMITS) {
          const v = adapter.limits?.[key];
          if (typeof v === "number" && Number.isFinite(v)) requiredLimits[key] = v;
        }
        device = await adapter.requestDevice({ requiredFeatures, requiredLimits });
        gpuAdapter = adapter;
        attachDeviceWatchers(device);
        return device;
      })().catch((err) => {
        devicePromise = null;
        throw err;
      });
    }
    return devicePromise;
  }

  function markDeviceLost(msg) {
    deviceLost = true;
    lastError = msg;
    device = null;
    devicePromise = null;
    gpuAdapter = null;
  }

  function attachDeviceWatchers(dev) {
    if (dev.lost && typeof dev.lost.then === "function") {
      dev.lost.then((info) => {
        const reason = info && info.reason ? info.reason : "unknown";
        const message = info && info.message ? info.message : "";
        markDeviceLost(`device lost (${reason}): ${message}`);
      }).catch(() => {
        markDeviceLost("device lost");
      });
    }
    if (typeof dev.addEventListener === "function") {
      dev.addEventListener("uncapturederror", (ev) => {
        if (ev && ev.preventDefault) ev.preventDefault();
        const err = ev && ev.error;
        const msg = String(err && err.message ? err.message : err || "uncaptured GPU error");
        lastError = msg;
        if (/device lost|lost device|parent device is lost/i.test(msg)) {
          markDeviceLost(msg);
        }
      });
    }
  }

  function attachFromAbi(abi, sourceHint) {
    gpuAbi = abi && abi.gpu ? abi.gpu : null;
    if (gpuAbi && typeof sourceHint === "string") {
      wgslSource = replaceArtifactExt(sourceHint, ".wgsl");
    }
  }

  function toU8(data) {
    return data instanceof Uint8Array ? data : Uint8Array.from(data || []);
  }

  function toI32Arr(data) {
    if (!data) return [];
    if (Array.isArray(data)) return data.map((x) => x | 0);
    return Array.from(data).map((x) => x | 0);
  }

  /** Packed surface input — keep in sync with native `gpu/input.rs`. */
  function makeInputState() {
    return {
      x: 0,
      y: 0,
      dx: 0,
      dy: 0,
      buttons: 0,
      inside: false,
      pointerId: -1,
      shift: false,
      ctrl: false,
      alt: false,
      meta: false,
      focused: true,
      closeRequested: false,
      keysDown: new Set(),
      pads: new Map(),
      knownPads: new Set(),
      pointers: new Map(),
      queue: [],
    };
  }

  const BTN_SOUTH = 1;
  const BTN_EAST = 2;
  const BTN_WEST = 3;
  const BTN_NORTH = 4;
  const BTN_DPAD_UP = 5;
  const BTN_DPAD_DOWN = 6;
  const BTN_DPAD_LEFT = 7;
  const BTN_DPAD_RIGHT = 8;
  const BTN_LEFT_SHOULDER = 9;
  const BTN_RIGHT_SHOULDER = 10;
  const BTN_LEFT_TRIGGER = 11;
  const BTN_RIGHT_TRIGGER = 12;
  const BTN_LEFT_STICK = 13;
  const BTN_RIGHT_STICK = 14;
  const BTN_START = 15;
  const BTN_SELECT = 16;
  const STICK_DEADZONE = 0.15;

  /** HTML Gamepad standard button index → Dream GamepadButton id. */
  const STANDARD_BUTTON_MAP = [
    BTN_SOUTH,
    BTN_EAST,
    BTN_WEST,
    BTN_NORTH,
    BTN_LEFT_SHOULDER,
    BTN_RIGHT_SHOULDER,
    BTN_LEFT_TRIGGER,
    BTN_RIGHT_TRIGGER,
    BTN_SELECT,
    BTN_START,
    BTN_LEFT_STICK,
    BTN_RIGHT_STICK,
    BTN_DPAD_UP,
    BTN_DPAD_DOWN,
    BTN_DPAD_LEFT,
    BTN_DPAD_RIGHT,
  ];

  function applyDeadzone(v) {
    return Math.abs(v) < STICK_DEADZONE ? 0 : Math.max(-1, Math.min(1, v));
  }

  function ensurePad(input, pad) {
    if (!input.pads.has(pad)) {
      input.pads.set(pad, {
        buttons: new Set(),
        axes: [0, 0, 0, 0, 0, 0],
      });
    }
    return input.pads.get(pad);
  }

  function syncGamepads(input) {
    if (typeof navigator === "undefined" || typeof navigator.getGamepads !== "function") {
      return;
    }
    let list;
    try {
      list = navigator.getGamepads();
    } catch (_) {
      return;
    }
    if (!list) return;
    const seen = new Set();
    for (let i = 0; i < list.length; i++) {
      const gp = list[i];
      if (!gp) continue;
      const pad = i | 0;
      seen.add(pad);
      if (!input.knownPads.has(pad)) {
        input.knownPads.add(pad);
        ensurePad(input, pad);
        pushEvent(input, { tag: 15, pad });
      }
      const state = ensurePad(input, pad);
      const pressed = new Set();
      const buttons = gp.buttons || [];
      for (let bi = 0; bi < buttons.length && bi < STANDARD_BUTTON_MAP.length; bi++) {
        const btn = buttons[bi];
        const down = !!(btn && (btn.pressed || (btn.value != null && btn.value > 0.5)));
        const id = STANDARD_BUTTON_MAP[bi];
        if (down) {
          pressed.add(id);
          if (!state.buttons.has(id)) {
            state.buttons.add(id);
            pushEvent(input, { tag: 17, pad, button: id });
          }
        }
      }
      for (const id of [...state.buttons]) {
        if (!pressed.has(id)) {
          state.buttons.delete(id);
          pushEvent(input, { tag: 18, pad, button: id });
        }
      }
      const axes = gp.axes || [];
      const lt = buttons[6];
      const rt = buttons[7];
      const nextAxes = [
        applyDeadzone(axes[0] || 0),
        applyDeadzone(axes[1] || 0),
        applyDeadzone(axes[2] || 0),
        applyDeadzone(axes[3] || 0),
        lt && lt.value != null ? Math.max(0, Math.min(1, lt.value)) : 0,
        rt && rt.value != null ? Math.max(0, Math.min(1, rt.value)) : 0,
      ];
      for (let ai = 0; ai < nextAxes.length; ai++) {
        const v = nextAxes[ai];
        if (Math.abs(v - (state.axes[ai] || 0)) > 1e-4) {
          pushEvent(input, { tag: 19, pad, axis: ai, value: v });
        }
        state.axes[ai] = v;
      }
    }
    for (const pad of [...input.knownPads]) {
      if (!seen.has(pad)) {
        input.knownPads.delete(pad);
        input.pads.delete(pad);
        pushEvent(input, { tag: 16, pad });
      }
    }
  }

  function pushEvent(input, ev) {
    if (input.queue.length >= 256) input.queue.shift();
    input.queue.push(ev);
  }

  function setPointerPos(input, x, y) {
    input.dx += x - input.x;
    input.dy += y - input.y;
    input.x = x;
    input.y = y;
  }

  function ensureTracked(input, pid) {
    if (!input.pointers.has(pid)) {
      input.pointers.set(pid, {
        x: input.x,
        y: input.y,
        dx: 0,
        dy: 0,
        buttons: 0,
        inside: false,
      });
    }
    return input.pointers.get(pid);
  }

  function trackPointerPos(input, pid, x, y, accumulate) {
    const p = ensureTracked(input, pid);
    if (accumulate) {
      p.dx += x - p.x;
      p.dy += y - p.y;
    }
    p.x = x;
    p.y = y;
    return p;
  }

  function effectivePixelRatio(surface) {
    const max = surface.maxPixelRatio != null ? Number(surface.maxPixelRatio) : 1;
    const dpr = globalThis.devicePixelRatio || 1;
    if (!(max > 1)) return 1;
    return Math.min(Math.max(1, dpr), max);
  }

  // Drawable follows CSS/client pixels unless `maxPixelRatio` opts into a clamped HiDPI store.
  function syncSurfaceClientSize(surface) {
    const canvas = surface.canvas;
    if (!canvas) return false;
    const cw = canvas.clientWidth | 0;
    const ch = canvas.clientHeight | 0;
    if (cw < 1 || ch < 1) return false;
    const ratio = effectivePixelRatio(surface);
    const dw = Math.max(1, Math.round(cw * ratio));
    const dh = Math.max(1, Math.round(ch * ratio));
    if (
      cw === surface.clientWidth &&
      ch === surface.clientHeight &&
      dw === surface.width &&
      dh === surface.height &&
      canvas.width === dw &&
      canvas.height === dh
    ) {
      return false;
    }
    surface.clientWidth = cw;
    surface.clientHeight = ch;
    surface.width = dw;
    surface.height = dh;
    surface.pixelRatio = ratio;
    surface.scaleFactor = globalThis.devicePixelRatio || 1;
    canvas.width = dw;
    canvas.height = dh;
    surface.configured = false;
    return true;
  }

  function canvasPointerPos(canvas, clientX, clientY) {
    const rect = canvas.getBoundingClientRect();
    const rw = rect.width || 1;
    const rh = rect.height || 1;
    const x = ((clientX - rect.left) / rw) * (canvas.width || 1);
    const y = ((clientY - rect.top) / rh) * (canvas.height || 1);
    return { x, y };
  }

  function writeF32(view, o, v) {
    view.setFloat32(o, v, true);
    return o + 4;
  }
  function writeI32(view, o, v) {
    view.setInt32(o, v | 0, true);
    return o + 4;
  }
  function appendF32(chunks, v) {
    const b = new ArrayBuffer(4);
    new DataView(b).setFloat32(0, v, true);
    chunks.push(new Uint8Array(b));
  }
  function appendI32(chunks, v) {
    const b = new ArrayBuffer(4);
    new DataView(b).setInt32(0, v | 0, true);
    chunks.push(new Uint8Array(b));
  }
  function appendStr(chunks, s) {
    const enc = new TextEncoder().encode(String(s ?? ""));
    appendI32(chunks, enc.length);
    chunks.push(enc);
  }
  function concatChunks(chunks) {
    let n = 0;
    for (const c of chunks) n += c.length;
    const out = new Uint8Array(n);
    let o = 0;
    for (const c of chunks) {
      out.set(c, o);
      o += c.length;
    }
    return out;
  }

  function packPointerRecord(view, o, rec) {
    o = writeF32(view, o, rec.x);
    o = writeF32(view, o, rec.y);
    o = writeF32(view, o, rec.dx);
    o = writeF32(view, o, rec.dy);
    o = writeI32(view, o, rec.buttons);
    o = writeI32(view, o, rec.buttons !== 0 ? 1 : 0);
    o = writeI32(view, o, rec.inside ? 1 : 0);
    writeI32(view, o, rec.pointerId | 0);
  }

  function packPointer(input) {
    const buf = new ArrayBuffer(32);
    const view = new DataView(buf);
    packPointerRecord(view, 0, {
      x: input.x,
      y: input.y,
      dx: input.dx,
      dy: input.dy,
      buttons: input.buttons,
      inside: input.inside,
      pointerId: input.pointerId,
    });
    input.dx = 0;
    input.dy = 0;
    return new Uint8Array(buf);
  }

  function packPointers(input) {
    const ids = [...input.pointers.keys()].sort((a, b) => a - b);
    const buf = new ArrayBuffer(4 + ids.length * 32);
    const view = new DataView(buf);
    let o = writeI32(view, 0, ids.length);
    for (const id of ids) {
      const p = input.pointers.get(id);
      packPointerRecord(view, o, {
        x: p.x,
        y: p.y,
        dx: p.dx,
        dy: p.dy,
        buttons: p.buttons,
        inside: p.inside,
        pointerId: id,
      });
      p.dx = 0;
      p.dy = 0;
      o += 32;
    }
    return new Uint8Array(buf);
  }

  function packMods(input) {
    return new Uint8Array([
      input.shift ? 1 : 0,
      input.ctrl ? 1 : 0,
      input.alt ? 1 : 0,
      input.meta ? 1 : 0,
    ]);
  }

  function packEvents(input) {
    syncGamepads(input);
    const chunks = [];
    appendI32(chunks, input.queue.length);
    for (const ev of input.queue) {
      chunks.push(new Uint8Array([ev.tag | 0]));
      switch (ev.tag) {
        case 0:
        case 1:
          appendF32(chunks, ev.x);
          appendF32(chunks, ev.y);
          appendI32(chunks, ev.button);
          appendI32(chunks, ev.pointerId);
          break;
        case 2:
        case 3:
        case 4:
          appendF32(chunks, ev.x);
          appendF32(chunks, ev.y);
          appendI32(chunks, ev.pointerId);
          break;
        case 5:
          appendI32(chunks, ev.pointerId);
          break;
        case 6:
          appendF32(chunks, ev.dx);
          appendF32(chunks, ev.dy);
          appendF32(chunks, ev.x);
          appendF32(chunks, ev.y);
          break;
        case 7:
          appendStr(chunks, ev.code);
          appendStr(chunks, ev.key);
          chunks.push(new Uint8Array([ev.repeat ? 1 : 0]));
          break;
        case 8:
          appendStr(chunks, ev.code);
          appendStr(chunks, ev.key);
          break;
        case 9:
          appendStr(chunks, ev.text);
          break;
        case 10:
          appendI32(chunks, ev.width);
          appendI32(chunks, ev.height);
          break;
        case 11:
          appendF32(chunks, ev.scale);
          break;
        case 15:
        case 16:
          appendI32(chunks, ev.pad);
          break;
        case 17:
        case 18:
          appendI32(chunks, ev.pad);
          chunks.push(new Uint8Array([ev.button | 0]));
          break;
        case 19:
          appendI32(chunks, ev.pad);
          chunks.push(new Uint8Array([ev.axis | 0]));
          appendF32(chunks, ev.value);
          break;
        default:
          break;
      }
    }
    input.queue = [];
    return concatChunks(chunks);
  }

  function attachSurfaceInput(surface) {
    const canvas = surface.canvas;
    const input = surface.input;
    if (!canvas || typeof canvas.addEventListener !== "function") return;

    const onPtr = (type, ev) => {
      const { x, y } = canvasPointerPos(canvas, ev.clientX, ev.clientY);
      const pid = ev.pointerId != null ? ev.pointerId | 0 : 0;
      const button = ev.button != null ? ev.button | 0 : 0;
      const locked = !!surface.pointerLocked;
      const mx = ev.movementX || 0;
      const my = ev.movementY || 0;
      if (type === "down") {
        setPointerPos(input, x, y);
        input.buttons |= 1 << Math.max(0, Math.min(31, button));
        input.pointerId = pid;
        const p = trackPointerPos(input, pid, x, y, true);
        p.buttons |= 1 << Math.max(0, Math.min(31, button));
        p.inside = true;
        pushEvent(input, { tag: 0, x, y, button, pointerId: pid });
        try {
          canvas.setPointerCapture?.(pid);
        } catch (_) {}
      } else if (type === "up") {
        setPointerPos(input, x, y);
        input.buttons &= ~(1 << Math.max(0, Math.min(31, button)));
        const p = trackPointerPos(input, pid, x, y, true);
        p.buttons &= ~(1 << Math.max(0, Math.min(31, button)));
        if (p.buttons === 0 && pid !== 0) input.pointers.delete(pid);
        pushEvent(input, { tag: 1, x, y, button, pointerId: pid });
      } else if (type === "move") {
        if (locked) {
          input.dx += mx;
          input.dy += my;
          input.x = x;
          input.y = y;
          const p = ensureTracked(input, pid);
          p.dx += mx;
          p.dy += my;
          p.x = x;
          p.y = y;
        } else {
          setPointerPos(input, x, y);
          trackPointerPos(input, pid, x, y, true);
        }
        input.pointerId = pid;
        pushEvent(input, { tag: 2, x, y, pointerId: pid });
      } else if (type === "enter") {
        input.inside = true;
        setPointerPos(input, x, y);
        input.pointerId = pid;
        const p = trackPointerPos(input, pid, x, y, true);
        p.inside = true;
        pushEvent(input, { tag: 3, x, y, pointerId: pid });
      } else if (type === "leave") {
        input.inside = false;
        setPointerPos(input, x, y);
        trackPointerPos(input, pid, x, y, true);
        input.pointers.delete(pid);
        pushEvent(input, { tag: 4, x, y, pointerId: pid });
      } else if (type === "cancel") {
        input.buttons = 0;
        input.pointers.delete(pid);
        pushEvent(input, { tag: 5, pointerId: pid });
      }
    };

    canvas.style.touchAction = "none";
    canvas.addEventListener("pointerdown", (e) => onPtr("down", e));
    canvas.addEventListener("pointerup", (e) => onPtr("up", e));
    canvas.addEventListener("pointermove", (e) => onPtr("move", e));
    canvas.addEventListener("pointerenter", (e) => onPtr("enter", e));
    canvas.addEventListener("pointerleave", (e) => onPtr("leave", e));
    canvas.addEventListener("pointercancel", (e) => onPtr("cancel", e));
    canvas.addEventListener(
      "wheel",
      (e) => {
        const { x, y } = canvasPointerPos(canvas, e.clientX, e.clientY);
        pushEvent(input, { tag: 6, dx: e.deltaX, dy: e.deltaY, x, y });
      },
      { passive: true },
    );

    const onKey = (down, e) => {
      input.shift = !!e.shiftKey;
      input.ctrl = !!e.ctrlKey;
      input.alt = !!e.altKey;
      input.meta = !!e.metaKey;
      const code = String(e.code || "");
      const key = String(e.key || "");
      if (down) {
        if (!e.repeat) input.keysDown.add(code);
        pushEvent(input, { tag: 7, code, key, repeat: !!e.repeat });
        if (!e.repeat && key.length === 1) {
          pushEvent(input, { tag: 9, text: key });
        }
      } else {
        input.keysDown.delete(code);
        pushEvent(input, { tag: 8, code, key });
      }
    };
    window.addEventListener("keydown", (e) => onKey(true, e));
    window.addEventListener("keyup", (e) => onKey(false, e));
    window.addEventListener("focus", () => {
      input.focused = true;
      pushEvent(input, { tag: 12 });
    });
    window.addEventListener("blur", () => {
      input.focused = false;
      pushEvent(input, { tag: 13 });
    });
    window.addEventListener("beforeunload", () => {
      input.closeRequested = true;
      pushEvent(input, { tag: 14 });
    });

    if (typeof ResizeObserver !== "undefined") {
      const ro = new ResizeObserver(() => {
        if (syncSurfaceClientSize(surface)) {
          pushEvent(input, { tag: 10, width: surface.clientWidth, height: surface.clientHeight });
        }
        const dpr = globalThis.devicePixelRatio || 1;
        surface.scaleFactor = dpr;
        pushEvent(input, { tag: 11, scale: dpr });
      });
      ro.observe(canvas);
    }

    if (typeof document !== "undefined" && typeof document.addEventListener === "function") {
      const onLockChange = () => {
        const locked = !!(document.pointerLockElement === canvas);
        surface.pointerLocked = locked;
      };
      const onFsChange = () => {
        const fsEl = document.fullscreenElement || document.webkitFullscreenElement;
        surface.fullscreen = fsEl === canvas;
      };
      document.addEventListener("pointerlockchange", onLockChange);
      document.addEventListener("fullscreenchange", onFsChange);
      document.addEventListener("webkitfullscreenchange", onFsChange);
    }
  }

  async function ensureBlit(dev) {
    if (blitPipeline) return;
    const code = `
struct VSOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f, };
@vertex fn vs(@builtin(vertex_index) vi: u32) -> VSOut {
  var positions = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
  var uvs = array<vec2f, 3>(vec2f(0.0, 1.0), vec2f(2.0, 1.0), vec2f(0.0, -1.0));
  var o: VSOut;
  o.pos = vec4f(positions[vi], 0.0, 1.0);
  o.uv = uvs[vi];
  return o;
}
@group(0) @binding(0) var samp: sampler;
@group(0) @binding(1) var tex: texture_2d<f32>;
@fragment fn fs(i: VSOut) -> @location(0) vec4f {
  return textureSample(tex, samp, i.uv);
}`;
    const module = dev.createShaderModule({ code });
    blitBindLayout = dev.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "filtering" } },
        { binding: 1, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "float" } },
      ],
    });
    blitPipeline = await dev.createRenderPipelineAsync({
      layout: dev.createPipelineLayout({ bindGroupLayouts: [blitBindLayout] }),
      vertex: { module, entryPoint: "vs" },
      fragment: {
        module,
        entryPoint: "fs",
        targets: [{ format: navigator.gpu.getPreferredCanvasFormat() }],
      },
      primitive: { topology: "triangle-list" },
    });
    blitSampler = dev.createSampler({ magFilter: "linear", minFilter: "linear" });
  }

  let loadWgslText = async (url) => {
    if (!url) throw new Error("no .wgsl URL; compile with Dream to emit sibling .wgsl");
    if (typeof fetch === "function") {
      const res = await fetch(url);
      if (!res.ok) throw new Error(`failed to fetch ${url}`);
      return await res.text();
    }
    throw new Error("fetch unavailable for .wgsl");
  };

  async function syncBufferToCpu(dev, b) {
    if (!b.gpuBuffer) return;
    const staging = dev.createBuffer({
      size: b.nbytes,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
    const encoder = dev.createCommandEncoder();
    encoder.copyBufferToBuffer(b.gpuBuffer, 0, staging, 0, b.nbytes);
    dev.queue.submit([encoder.finish()]);
    await staging.mapAsync(GPUMapMode.READ);
    const copy = staging.getMappedRange().slice(0);
    staging.unmap();
    staging.destroy();
    b.cpu = new Uint8Array(copy);
  }

  async function ensureGpuBuffer(dev, b, extraUsage = 0) {
    const need =
      GPUBufferUsage.STORAGE |
      GPUBufferUsage.COPY_DST |
      GPUBufferUsage.COPY_SRC |
      GPUBufferUsage.INDIRECT |
      extraUsage;
    if (b.gpuBuffer && (b.usage | 0) === (need | 0)) return b.gpuBuffer;
    if (b.gpuBuffer) {
      // Recreate with broader usage if needed.
      await syncBufferToCpu(dev, b);
      b.gpuBuffer.destroy();
      b.gpuBuffer = null;
    }
    b.usage = need;
    b.gpuBuffer = dev.createBuffer({
      size: Math.max(4, b.nbytes),
      usage: need,
    });
    if (b.cpu) {
      const bytes = b.cpu instanceof Uint8Array
        ? b.cpu
        : new Uint8Array(b.cpu.buffer, b.cpu.byteOffset, b.cpu.byteLength);
      dev.queue.writeBuffer(b.gpuBuffer, 0, bytes);
    }
    return b.gpuBuffer;
  }

  /// Usage flags for a texture. Dream does not ask callers to declare usage, so every texture gets
  /// everything its format and shape can legally support — WebGPU rejects flags a format cannot
  /// honor, so the set has to be narrowed rather than always maximal.
  function textureUsage(t) {
    let usage = GPUTextureUsage.TEXTURE_BINDING;
    // Multisampled textures cannot be the source or destination of a copy.
    if ((t.sample_count | 0) <= 1) {
      usage |= GPUTextureUsage.COPY_DST | GPUTextureUsage.COPY_SRC;
    }
    // Block-compressed and 1d/3d textures cannot be rendered into.
    if (!(t.spec && t.spec.block) && (t.dimension || "2d") === "2d") {
      usage |= GPUTextureUsage.RENDER_ATTACHMENT;
    }
    if (t.storage) usage |= GPUTextureUsage.STORAGE_BINDING;
    return usage;
  }

  async function ensureTexture(dev, t, storage) {
    // A texture created without a storage access but bound to a `texture_storage_*` slot needs
    // STORAGE_BINDING, which cannot be added after the fact — drop and recreate.
    if (storage && !t.storage) {
      t.storage = true;
      if (t.texture) {
        try { t.texture.destroy(); } catch (_) {}
        t.texture = null;
      }
    }
    if (t.texture) return t.texture;
    t.texture = dev.createTexture({
      size: [t.width, t.height, t.depth_or_layers || 1],
      format: t.format,
      usage: textureUsage(t),
      dimension: t.dimension || "2d",
      mipLevelCount: Math.max(1, t.mip_levels | 0 || 1),
      sampleCount: Math.max(1, t.sample_count | 0 || 1),
    });
    if (t.cpu) {
      // `cpu` only exists for linearly copyable color formats, so this upload is always valid.
      // Every layer goes up: a cubemap's five other faces are in the mirror too.
      const bpp = t.spec ? t.spec.bytesPerTexel : 4;
      dev.queue.writeTexture(
        { texture: t.texture },
        t.cpu,
        { bytesPerRow: t.width * bpp, rowsPerImage: t.height },
        [t.width, t.height, t.depth_or_layers || 1],
      );
    }
    return t.texture;
  }

  async function ensureSampler(dev, s) {
    if (s.sampler) return s.sampler;
    // WebGPU requires all three filters to be linear before anisotropy takes effect, and rejects
    // an anisotropy above 1 otherwise.
    const trilinear =
      s.magFilter === "linear" && s.minFilter === "linear" && s.mipmapFilter === "linear";
    s.sampler = dev.createSampler({
      magFilter: s.magFilter,
      minFilter: s.minFilter,
      mipmapFilter: s.mipmapFilter,
      addressModeU: s.addressModeU,
      addressModeV: s.addressModeV,
      addressModeW: s.addressModeW,
      lodMinClamp: s.lodMinClamp,
      lodMaxClamp: s.lodMaxClamp,
      compare: s.compare,
      maxAnisotropy: trilinear ? s.maxAnisotropy : 1,
    });
    return s.sampler;
  }

  // WGSL binding indices are unique per `@group`, so bindings are grouped and each group gets its
  // own layout. `(group, binding)` pairs are deduped: a vertex and fragment stage declaring the
  // same slot describe one resource.
  function planGroups(binds) {
    const seen = new Set();
    const byGroup = new Map();
    for (const b of binds || []) {
      const group = b.group | 0;
      const key = `${group}\0${b.binding | 0}`;
      if (seen.has(key)) continue;
      seen.add(key);
      if (!byGroup.has(group)) byGroup.set(group, []);
      byGroup.get(group).push(b);
    }
    return [...byGroup.entries()]
      .sort((a, b) => a[0] - b[0])
      .map(([group, bindings]) => ({ group, bindings }));
  }

  /// `uniformSize` is the declared block size, which the uniform entry needs as its
  /// `minBindingSize`: the block is a window into a shared ring reached by dynamic offset, and a
  /// window with no declared size would run to the end of the ring.
  function layoutEntryForBinding(b, visibility, uniformSize) {
    const base = { binding: b.binding, visibility };
    if (b.kind === "uniform") {
      return {
        ...base,
        buffer: {
          type: "uniform",
          hasDynamicOffset: true,
          minBindingSize: uniformSize | 0,
        },
      };
    }
    if (b.kind === "storage") {
      return {
        ...base,
        buffer: { type: b.read_write ? "storage" : "read-only-storage" },
      };
    }
    if (b.kind === "sampler") {
      return {
        ...base,
        sampler: { type: b.sample_type === "comparison" ? "comparison" : "filtering" },
      };
    }
    if (b.kind === "storage_texture") {
      return {
        ...base,
        storageTexture: {
          access: b.storage_access || "write-only",
          format: b.storage_format || "rgba8unorm",
          viewDimension: b.view_dimension || "2d",
        },
      };
    }
    // Sampled texture. The emitter records the shape the shader declared, so nothing here has to
    // guess at a 2d/float default.
    return {
      ...base,
      texture: {
        sampleType: b.sample_type || "float",
        viewDimension: b.view_dimension || "2d",
        multisampled: !!b.multisampled,
      },
    };
  }

  /// Dense per-group layouts for a pipeline layout; unused group indices get an empty layout so
  /// the array stays contiguous (a shader using groups 0 and 2 still needs a slot for 1).
  function createGroupLayouts(dev, plans, visibility, uniformSize) {
    if (!plans.length) return [];
    const maxGroup = Math.max(...plans.map((p) => p.group));
    const out = [];
    for (let g = 0; g <= maxGroup; g++) {
      const plan = plans.find((p) => p.group === g);
      const entries = plan
        ? plan.bindings.map((b) => layoutEntryForBinding(b, visibility, uniformSize))
        : [];
      out.push(dev.createBindGroupLayout({ entries }));
    }
    return out;
  }

  async function getPipeline(dev, kernel) {
    const meta = (gpuAbi && gpuAbi.kernels || []).find((k) => k.name === kernel);
    if (!meta) throw new Error(`unknown @compute kernel '${kernel}'`);
    let pipe = pipelineCache.get(kernel);
    if (pipe) return pipe;
    const code = (typeof meta.source === "string" && meta.source.length > 0)
      ? meta.source
      : await loadWgslText(wgslSource);
    const module = dev.createShaderModule({ code });
    if (typeof module.getCompilationInfo === "function") {
      const info = await module.getCompilationInfo();
      const errs = (info.messages || []).filter((m) => m.type === "error");
      if (errs.length) {
        throw new Error(`WGSL compile error in kernel '${kernel}':\n` +
          errs.map((m) => `${m.message} @${m.lineNum}:${m.linePos}`).join("\n"));
      }
    }
    const groups = planGroups(meta.bindings);
    const layouts = createGroupLayouts(dev, groups, GPUShaderStage.COMPUTE, meta.uniform_size | 0);
    const pipeline = await dev.createComputePipelineAsync({
      layout: dev.createPipelineLayout({ bindGroupLayouts: layouts }),
      compute: { module, entryPoint: meta.entry },
    });
    pipe = { pipeline, layouts, groups, meta };
    pipelineCache.set(kernel, pipe);
    return pipe;
  }

  /// Bump allocator for per-draw uniform blocks, mirroring the native `uniform_ring`.
  ///
  /// Every `set_uniforms` needs its own storage or batched draws would all read whichever write
  /// landed last. One buffer serves the whole frame and each draw binds a window of it at a
  /// dynamic offset, which both lifts any cap on the block size and lets a bind group outlive the
  /// draw that built it — it no longer depends on which slot the draw was given.
  const UNIFORM_RING_INITIAL = 64 * 1024;
  const uniformRing = { buffer: null, capacity: 0, cursor: 0, alignment: 256 };

  function uniformStride(size, dev) {
    // A reservation is computed before `beginUniformFrame` has recorded the device alignment, so
    // the device is consulted directly when one is at hand.
    const align = (dev && dev.limits && dev.limits.minUniformBufferOffsetAlignment)
      || uniformRing.alignment
      || 256;
    return Math.ceil(Math.max(size, 1) / align) * align;
  }

  /// Reserves `bytes` for the frame ahead. Returns whether the buffer was replaced, which
  /// invalidates every bind group built against the old one — growing partway through a frame
  /// would leave cached groups reading a buffer the new offsets were never written to.
  function beginUniformFrame(dev, bytes) {
    uniformRing.cursor = 0;
    const limit = dev.limits && dev.limits.minUniformBufferOffsetAlignment;
    uniformRing.alignment = limit || 256;
    if (bytes <= uniformRing.capacity) return false;
    let capacity = Math.max(bytes, UNIFORM_RING_INITIAL);
    capacity = Math.pow(2, Math.ceil(Math.log2(capacity)));
    uniformRing.buffer = dev.createBuffer({
      size: capacity,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    uniformRing.capacity = capacity;
    return true;
  }

  /// Writes one block and returns the dynamic offset to bind it at. Short blocks are padded up to
  /// `size`; a block longer than the shader declares means the Dream and WGSL layouts disagree.
  function pushUniforms(dev, bytes, size) {
    if (bytes.byteLength > size) {
      throw new Error(
        `packed ${bytes.byteLength} bytes of uniforms but the shader's block is ${size} bytes`,
      );
    }
    const stride = uniformStride(size);
    const offset = uniformRing.cursor;
    if (!uniformRing.buffer || offset + stride > uniformRing.capacity) {
      throw new Error("uniform ring was not reserved for this frame");
    }
    uniformRing.cursor = offset + stride;
    const padded = new Uint8Array(size);
    if (bytes.byteLength > 0) padded.set(bytes, 0);
    dev.queue.writeBuffer(uniformRing.buffer, offset, padded);
    return offset;
  }

  /// Resolves the resource id arrays into one bind group per `@group`. Ids are consumed
  /// positionally per kind, which is the order `GpuBindList` appends them in.
  async function buildBindGroup(dev, pipe, bufferIds, textureIds, samplerIds, uniforms) {
    const bufIds = toI32Arr(bufferIds);
    const texIds = toI32Arr(textureIds);
    const sampIds = toI32Arr(samplerIds);
    let storageIdx = 0;
    let textureIdx = 0;
    let samplerIdx = 0;
    const extra = toU8(uniforms);
    const size = pipe.meta.uniform_size | 0;
    const offsets = new Map();
    const out = [];
    for (const { group, bindings } of pipe.groups) {
      const resources = [];
      for (const bind of bindings) {
        if (bind.kind === "uniform") {
          offsets.set(group, pushUniforms(dev, extra, size));
          resources.push({
            binding: bind.binding,
            resource: { buffer: uniformRing.buffer, offset: 0, size },
          });
        } else if (bind.kind === "storage") {
          const id = bufIds[storageIdx++] | 0;
          const b = buffers.get(id);
          if (!b) throw new Error(`missing buffer id ${id} for binding ${bind.binding}`);
          const gpuBuf = await ensureGpuBuffer(dev, b);
          resources.push({ binding: bind.binding, resource: { buffer: gpuBuf } });
        } else if (bind.kind === "sampler") {
          const id = sampIds[samplerIdx++] | 0;
          const s = samplers.get(id);
          if (!s) throw new Error(`missing sampler id ${id} for binding ${bind.binding}`);
          resources.push({ binding: bind.binding, resource: await ensureSampler(dev, s) });
        } else {
          const id = texIds[textureIdx++] | 0;
          const t = textures.get(id);
          if (!t) throw new Error(`missing texture id ${id} for binding ${bind.binding}`);
          const tex = await ensureTexture(dev, t, bind.kind === "storage_texture");
          resources.push({ binding: bind.binding, resource: textureViewFor(t, tex) });
        }
      }
      out.push({
        group,
        bindGroup: dev.createBindGroup({ layout: pipe.layouts[group], entries: resources }),
        offsets: offsets.has(group) ? [offsets.get(group)] : [],
      });
    }
    return out;
  }

  /// Views a texture as its declared view dimension. A 6-layer 2D texture defaults to `2d-array`,
  /// which cannot fill a `texture_cube` slot.
  function textureViewFor(t, tex) {
    return tex.createView({ dimension: t.viewDimension || "2d" });
  }

  function setBindGroups(pass, groups) {
    for (const { group, bindGroup, offsets } of groups) {
      pass.setBindGroup(group, bindGroup, offsets || []);
    }
  }

  function encodeDispatchInto(pass, pipe, bgs, ex, ey, ez) {
    const wg = pipe.meta.workgroup || [64, 1, 1];
    const gx = Math.max(1, Math.ceil((ex | 0) / (wg[0] || 64)));
    const gy = Math.max(1, Math.ceil((ey | 0) / (wg[1] || 1)));
    const gz = Math.max(1, Math.ceil((ez | 0) / (wg[2] || 1)));
    pass.setPipeline(pipe.pipeline);
    setBindGroups(pass, bgs);
    pass.dispatchWorkgroups(gx, gy, gz);
  }

  function encodeDispatch(encoder, pipe, bgs, ex, ey, ez) {
    const pass = encoder.beginComputePass();
    encodeDispatchInto(pass, pipe, bgs, ex, ey, ez);
    pass.end();
  }

  async function encodeDispatchIndirectInto(dev, pass, pipe, bgs, indirectId, offset) {
    const b = buffers.get(indirectId);
    if (!b) throw new Error(`missing indirect buffer ${indirectId}`);
    const gpuBuf = await ensureGpuBuffer(dev, b);
    pass.setPipeline(pipe.pipeline);
    setBindGroups(pass, bgs);
    pass.dispatchWorkgroupsIndirect(gpuBuf, Math.max(0, offset | 0));
  }

  async function encodeDispatchIndirect(dev, encoder, pipe, bgs, indirectId, offset) {
    const pass = encoder.beginComputePass();
    await encodeDispatchIndirectInto(dev, pass, pipe, bgs, indirectId, offset);
    pass.end();
  }

  async function runDispatch(kernel, bufferIds, textureIds, samplerIds, ex, ey, ez, uniforms) {
    const dev = await ensureDevice();
    const pipe = await getPipeline(dev, kernel);
    beginUniformFrame(dev, uniformStride(pipe.meta.uniform_size | 0, dev));
    const bg = await buildBindGroup(
      dev, pipe, bufferIds, textureIds, samplerIds, uniforms,
    );
    const encoder = dev.createCommandEncoder();
    encodeDispatch(encoder, pipe, bg, ex, ey, ez);
    dev.queue.submit([encoder.finish()]);
    await dev.queue.onSubmittedWorkDone();
    return 0;
  }

  async function runDispatchIndirect(
    kernel, bufferIds, textureIds, samplerIds, indirectId, indirectOffset,
  ) {
    const dev = await ensureDevice();
    const pipe = await getPipeline(dev, kernel);
    beginUniformFrame(dev, uniformStride(pipe.meta.uniform_size | 0, dev));
    const bg = await buildBindGroup(
      dev, pipe, bufferIds, textureIds, samplerIds, [],
    );
    const encoder = dev.createCommandEncoder();
    await encodeDispatchIndirect(dev, encoder, pipe, bg, indirectId, indirectOffset);
    dev.queue.submit([encoder.finish()]);
    await dev.queue.onSubmittedWorkDone();
    return 0;
  }

  /// Number of buffer / texture / sampler ids a group consumes from a bind list.
  function groupArity(bindings) {
    let bufs = 0, texs = 0, samps = 0;
    for (const b of bindings) {
      if (b.kind === "uniform") continue;
      else if (b.kind === "storage") bufs++;
      else if (b.kind === "sampler") samps++;
      else texs++;
    }
    return { bufs, texs, samps };
  }

  /// One bind group per `@group` declared by the VS/FS pair.
  ///
  /// A group named by `set_bind_group` uses that handle's pinned resources; the rest draw from the
  /// `set_bind_list` ids, in group order, consumed positionally per kind.
  async function buildRenderBindGroups(dev, rp, pipelineId, explicit, list, uniformOffset) {
    if (!rp.groups.length) return [];
    const bufIds = list ? list.buffers : [];
    const texIds = list ? list.textures : [];
    const sampIds = list ? list.samplers : [];
    let storageIdx = 0;
    let textureIdx = 0;
    let samplerIdx = 0;
    const out = [];
    for (const { group, bindings } of rp.groups) {
      const handleId = explicit.has(group) ? explicit.get(group) : -1;
      const hasUniform = bindings.some((b) => b.kind === "uniform");
      if (hasUniform && uniformOffset == null) {
        throw new Error(
          `@group(${group}) needs a uniform block; call set_uniforms before the draw`,
        );
      }
      const offsets = hasUniform ? [uniformOffset] : [];
      let pinned = null;
      if (handleId >= 0) {
        pinned = bindGroups.get(handleId);
        if (!pinned) throw new Error(`unknown bind group ${handleId}`);
        if (pinned.pipelineId !== pipelineId || pinned.group !== group) {
          throw new Error(
            `bind group ${handleId} was built for pipeline ${pinned.pipelineId} @group(${pinned.group}), ` +
            `not pipeline ${pipelineId} @group(${group})`,
          );
        }
        const cached = renderBgCache.get(`${pipelineId}:${group}:${handleId}`);
        if (cached) {
          out.push({ group, bindGroup: cached, offsets });
          continue;
        }
      }
      // A pinned handle brings its own ids and must not consume from the list, or the groups
      // after it would shift.
      const take = pinned
        ? { b: pinned.bufferIds, t: pinned.textureIds, s: pinned.samplerIds, bi: 0, ti: 0, si: 0 }
        : null;
      const nextBuf = () => (take ? take.b[take.bi++] | 0 : bufIds[storageIdx++] | 0);
      const nextTex = () => (take ? take.t[take.ti++] | 0 : texIds[textureIdx++] | 0);
      const nextSamp = () => (take ? take.s[take.si++] | 0 : sampIds[samplerIdx++] | 0);
      const entries = [];
      for (const bind of bindings) {
        if (bind.kind === "uniform") {
          // The window starts at 0 and the draw's dynamic offset moves it, so one group serves
          // every draw regardless of which slice of the ring that draw wrote.
          entries.push({
            binding: bind.binding,
            resource: { buffer: uniformRing.buffer, offset: 0, size: rp.uniformSize },
          });
        } else if (bind.kind === "storage") {
          const id = nextBuf();
          const b = buffers.get(id);
          if (!b) throw new Error(`missing buffer id ${id} for @group(${group}) @binding(${bind.binding})`);
          const gpuBuf = await ensureGpuBuffer(dev, b, GPUBufferUsage.STORAGE);
          entries.push({ binding: bind.binding, resource: { buffer: gpuBuf } });
        } else if (bind.kind === "sampler") {
          const id = nextSamp();
          const s = samplers.get(id);
          if (!s) throw new Error(`missing sampler id ${id} for @group(${group}) @binding(${bind.binding})`);
          entries.push({ binding: bind.binding, resource: await ensureSampler(dev, s) });
        } else {
          const id = nextTex();
          const t = textures.get(id);
          if (!t) throw new Error(`missing texture id ${id} for @group(${group}) @binding(${bind.binding})`);
          const tex = await ensureTexture(dev, t, bind.kind === "storage_texture");
          entries.push({ binding: bind.binding, resource: textureViewFor(t, tex) });
        }
      }
      const bindGroup = dev.createBindGroup({ layout: rp.layouts[group], entries });
      // Only pinned handles are cached: a bind list's resources are whatever the caller passed
      // this frame, so caching them would be wrong the moment they change.
      if (handleId >= 0) {
        renderBgCache.set(`${pipelineId}:${group}:${handleId}`, bindGroup);
      }
      out.push({ group, bindGroup, offsets });
    }
    return out;
  }

  // --- Recorded command streams ---------------------------------------------------------------
  // Wire format is documented once, in `crates/dream-stdlib/src/system/gpu/gpu_cmd.dream`.

  const CMD_VERSION = 1;

  function decodeStream(bytes) {
    const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    let pos = 0;
    const i32 = () => {
      if (pos + 4 > dv.byteLength) throw new Error("command stream truncated");
      const v = dv.getInt32(pos, true);
      pos += 4;
      return v;
    };
    const f32 = () => {
      if (pos + 4 > dv.byteLength) throw new Error("command stream truncated");
      const v = dv.getFloat32(pos, true);
      pos += 4;
      return v;
    };
    const arr = () => {
      const n = Math.max(0, i32());
      const out = new Array(n);
      for (let i = 0; i < n; i++) out[i] = i32();
      return out;
    };
    const blob = () => {
      const n = Math.max(0, i32());
      const padded = (n + 3) & ~3;
      if (pos + padded > dv.byteLength) throw new Error("command stream blob truncated");
      const out = bytes.subarray(pos, pos + n);
      pos += padded;
      return out;
    };

    const version = i32();
    if (version !== CMD_VERSION) {
      throw new Error(`unsupported command stream version ${version} (host understands ${CMD_VERSION})`);
    }
    const records = [];
    while (pos < dv.byteLength) {
      const op = i32();
      switch (op) {
        case 1: {
          const colorCount = Math.max(0, i32());
          const desc = {
            depthId: i32(),
            depthLoad: i32(),
            depthStore: i32(),
            depthClear: f32(),
            stencilLoad: i32(),
            stencilStore: i32(),
            stencilClear: i32(),
            colors: [],
          };
          for (let i = 0; i < colorCount; i++) {
            desc.colors.push({
              kind: i32(),
              id: i32(),
              resolveId: i32(),
              load: i32(),
              store: i32(),
              clear: [f32(), f32(), f32(), f32()],
            });
          }
          records.push({ op: "beginPass", desc });
          break;
        }
        case 2: records.push({ op: "endPass" }); break;
        case 3: records.push({ op: "setPipeline", id: i32() }); break;
        case 4: records.push({ op: "setBindGroup", group: i32(), id: i32() }); break;
        case 5: records.push({ op: "setUniforms", bytes: blob() }); break;
        case 6: records.push({ op: "setVertexBuffer", slot: i32(), buffer: i32() }); break;
        case 7: records.push({ op: "setIndexBuffer", buffer: i32(), fmt: i32() }); break;
        case 8:
          records.push({
            op: "setViewport",
            x: f32(), y: f32(), w: f32(), h: f32(), minDepth: f32(), maxDepth: f32(),
          });
          break;
        case 9: records.push({ op: "setScissor", x: i32(), y: i32(), w: i32(), h: i32() }); break;
        case 10:
          records.push({
            op: "draw",
            vertexCount: i32(), instanceCount: i32(), firstVertex: i32(), firstInstance: i32(),
          });
          break;
        case 11:
          records.push({
            op: "drawIndexed",
            indexCount: i32(), instanceCount: i32(), firstIndex: i32(),
            baseVertex: i32(), firstInstance: i32(),
          });
          break;
        case 12: records.push({ op: "drawIndirect", buffer: i32(), offset: i32() }); break;
        case 13: records.push({ op: "drawIndexedIndirect", buffer: i32(), offset: i32() }); break;
        case 14:
          records.push({ op: "setBindList", buffers: arr(), textures: arr(), samplers: arr() });
          break;
        default: throw new Error(`unknown command stream opcode ${op}`);
      }
    }
    return records;
  }

  function ensureCanvasContext(dev, s) {
    if (!s.context) {
      s.context = s.canvas.getContext("webgpu");
      if (!s.context) throw new Error("canvas webgpu context unavailable");
    }
    if (!s.configured) {
      s.context.configure({
        device: dev,
        format: navigator.gpu.getPreferredCanvasFormat(),
        alphaMode: s.alphaMode || "opaque",
        colorSpace: s.colorSpace || "srgb",
      });
      s.configured = true;
    }
    return s.context;
  }

  /// Multisampled color target for a canvas pass, resolved into the swapchain view.
  function ensureSurfaceMsaa(dev, s, format, samples) {
    if (!s.msaa || s.msaaSamples !== samples || s.msaaWidth !== s.canvas.width ||
        s.msaaHeight !== s.canvas.height) {
      s.msaa = dev.createTexture({
        size: [Math.max(1, s.canvas.width), Math.max(1, s.canvas.height)],
        format,
        sampleCount: samples,
        usage: GPUTextureUsage.RENDER_ATTACHMENT,
      });
      s.msaaSamples = samples;
      s.msaaWidth = s.canvas.width;
      s.msaaHeight = s.canvas.height;
    }
    return s.msaa.createView();
  }

  function ensureSurfaceDepth(dev, s, samples) {
    if (!s.depthTex || s.depthSamples !== samples || s.depthWidth !== s.canvas.width ||
        s.depthHeight !== s.canvas.height) {
      s.depthTex = dev.createTexture({
        size: [Math.max(1, s.canvas.width), Math.max(1, s.canvas.height)],
        format: "depth24plus",
        sampleCount: samples,
        usage: GPUTextureUsage.RENDER_ATTACHMENT,
      });
      s.depthSamples = samples;
      s.depthWidth = s.canvas.width;
      s.depthHeight = s.canvas.height;
    }
    return s.depthTex.createView();
  }

  /// A `GpuTexture` used as a color attachment needs RENDER_ATTACHMENT usage, which `ensureTexture`
  /// doesn't set; recreate up-front if it's missing (cheap — draw targets are stable).
  function ensureRenderTarget(dev, t) {
    if (!t.texture) {
      const format = t.format || "rgba8unorm";
      t.texture = dev.createTexture({
        size: [t.width, t.height, t.depth_or_layers || 1],
        format,
        usage:
          GPUTextureUsage.TEXTURE_BINDING |
          GPUTextureUsage.RENDER_ATTACHMENT |
          GPUTextureUsage.COPY_DST |
          GPUTextureUsage.COPY_SRC |
          (format === "rgba8unorm" ? GPUTextureUsage.STORAGE_BINDING : 0),
        mipLevelCount: Math.max(1, (t.mip_levels | 0) || 1),
      });
      if (t.cpu) {
        const bpp = format === "rgba16float" ? 8 : 4;
        dev.queue.writeTexture(
          { texture: t.texture },
          t.cpu,
          { bytesPerRow: t.width * bpp },
          [t.width, t.height],
        );
      }
    }
    return t.texture;
  }

  async function resolveAttachments(dev, desc, sampleCount) {
    if (!desc.colors.length) throw new Error("render pass has no color attachments");
    const colors = [];
    const touched = [];
    let format = null;
    let surfaceId = -1;
    for (const c of desc.colors) {
      let view, resolveTarget = undefined, fmt;
      if (c.kind === 0) {
        const s = surfaces.get(c.id);
        if (!s) throw new Error(`unknown GpuSurface ${c.id}`);
        surfaceId = c.id;
        fmt = navigator.gpu.getPreferredCanvasFormat();
        const single = ensureCanvasContext(dev, s).getCurrentTexture().createView();
        if (sampleCount > 1) {
          view = ensureSurfaceMsaa(dev, s, fmt, sampleCount);
          resolveTarget = single;
        } else {
          view = single;
        }
      } else if (c.kind === 1) {
        const t = textures.get(c.id);
        if (!t) throw new Error(`unknown color GpuTexture ${c.id}`);
        if (t.depth) throw new Error("validation: color target cannot be a depth texture");
        if (sampleCount > 1) {
          throw new Error(
            `texture ${c.id} cannot be an MSAA attachment: offscreen multisampling needs a multisampled texture`,
          );
        }
        fmt = t.format || "rgba8unorm";
        view = ensureRenderTarget(dev, t).createView();
        touched.push(t);
        if (c.resolveId >= 0) {
          const rt = textures.get(c.resolveId);
          if (!rt) throw new Error(`unknown resolve GpuTexture ${c.resolveId}`);
          resolveTarget = ensureRenderTarget(dev, rt).createView();
          touched.push(rt);
        }
      } else {
        throw new Error(`unknown color attachment target ${c.kind}`);
      }
      if (format == null) format = fmt;
      colors.push({
        view,
        resolveTarget,
        clearValue: { r: c.clear[0], g: c.clear[1], b: c.clear[2], a: c.clear[3] },
        loadOp: c.load === 1 ? "load" : "clear",
        storeOp: c.store === 1 ? "discard" : "store",
      });
    }

    const passDesc = { colorAttachments: colors };
    let depthView = null;
    if (desc.depthId === -2) {
      if (surfaceId < 0) throw new Error("surface_depth() needs a surface color attachment");
      depthView = ensureSurfaceDepth(dev, surfaces.get(surfaceId), sampleCount);
    } else if (desc.depthId >= 0) {
      const dt = textures.get(desc.depthId);
      if (!dt) throw new Error(`unknown depth GpuTexture ${desc.depthId}`);
      if (!dt.depth) {
        throw new Error(
          `validation: GpuTexture ${desc.depthId} is ${dt.format}, not a depth format`,
        );
      }
      depthView = (await ensureTexture(dev, dt, false)).createView();
    } else if (desc.depthId !== -1) {
      throw new Error(`unknown depth attachment target ${desc.depthId}`);
    }
    if (depthView) {
      passDesc.depthStencilAttachment = {
        view: depthView,
        depthClearValue: desc.depthClear,
        depthLoadOp: desc.depthLoad === 1 ? "load" : "clear",
        depthStoreOp: desc.depthStore === 1 ? "discard" : "store",
      };
    }
    return { passDesc, format, touched };
  }

  /// Resolves one pass's resources into a flat step list, then replays it.
  ///
  /// Resources are resolved before `beginRenderPass` so no `await` lands inside the pass, where an
  /// interleaved host call could record into it.
  async function replayPass(dev, encoder, desc, body) {
    const firstPipeline = body.find((r) => r.op === "setPipeline");
    if (!firstPipeline) throw new Error("render pass issued no set_pipeline");
    const firstRp = renderPipelines.get(firstPipeline.id);
    if (!firstRp) throw new Error(`unknown GpuRenderPipeline ${firstPipeline.id}`);
    const { passDesc, touched } = await resolveAttachments(dev, desc, firstRp.sampleCount || 1);

    const steps = [];
    let pipelineId = -1;
    let rp = null;
    let explicit = new Map();
    let list = null;
    let uniformOffset = null;
    for (const rec of body) {
      switch (rec.op) {
        case "setPipeline": {
          rp = renderPipelines.get(rec.id);
          if (!rp) throw new Error(`unknown GpuRenderPipeline ${rec.id}`);
          pipelineId = rec.id;
          // A pipeline switch invalidates the previous pipeline's group layouts.
          explicit = new Map();
          list = null;
          steps.push({ op: "pipeline", pipeline: rp.pipeline });
          break;
        }
        case "setBindGroup": explicit.set(rec.group, rec.id); break;
        case "setBindList": list = rec; break;
        case "setUniforms": {
          if (!rp) throw new Error("set_uniforms before set_pipeline");
          const size = rp.uniformSize | 0;
          // The one-draw `GpuRenderPass` helpers record `set_uniforms` unconditionally, so an
          // empty blob against a shader that takes none is ordinary — but packed bytes with
          // nowhere to land mean the caller and the shader disagree.
          if (size === 0) {
            if (toU8(rec.bytes).byteLength > 0) {
              throw new Error(
                `packed uniforms but pipeline ${pipelineId} declares no uniform block`,
              );
            }
            uniformOffset = null;
          } else {
            uniformOffset = pushUniforms(dev, toU8(rec.bytes), size);
          }
          break;
        }
        case "setVertexBuffer": {
          const b = buffers.get(rec.buffer);
          if (!b) throw new Error(`unknown vertex GpuBuffer ${rec.buffer}`);
          steps.push({
            op: "vertexBuffer",
            slot: rec.slot,
            buffer: await ensureGpuBuffer(dev, b, GPUBufferUsage.VERTEX),
          });
          break;
        }
        case "setIndexBuffer": {
          const b = buffers.get(rec.buffer);
          if (!b) throw new Error(`unknown index GpuBuffer ${rec.buffer}`);
          steps.push({
            op: "indexBuffer",
            buffer: await ensureGpuBuffer(dev, b, GPUBufferUsage.INDEX),
            fmt: rec.fmt === 1 ? "uint16" : "uint32",
          });
          break;
        }
        case "setViewport":
        case "setScissor":
          steps.push(rec);
          break;
        case "draw":
        case "drawIndexed":
        case "drawIndirect":
        case "drawIndexedIndirect": {
          if (!rp) throw new Error("draw before set_pipeline");
          const bgs = await buildRenderBindGroups(dev, rp, pipelineId, explicit, list, uniformOffset);
          if (bgs.length) steps.push({ op: "bindGroups", bgs });
          if (rec.op === "drawIndirect" || rec.op === "drawIndexedIndirect") {
            const b = buffers.get(rec.buffer);
            if (!b) throw new Error(`unknown indirect GpuBuffer ${rec.buffer}`);
            steps.push({
              op: rec.op,
              buffer: await ensureGpuBuffer(dev, b, GPUBufferUsage.INDIRECT),
              offset: Math.max(0, rec.offset | 0),
            });
          } else {
            steps.push(rec);
          }
          break;
        }
        default: throw new Error(`unexpected command '${rec.op}' inside a render pass`);
      }
    }

    const pass = encoder.beginRenderPass(passDesc);
    for (const step of steps) {
      switch (step.op) {
        case "pipeline": pass.setPipeline(step.pipeline); break;
        case "bindGroups": setBindGroups(pass, step.bgs); break;
        case "vertexBuffer": pass.setVertexBuffer(step.slot, step.buffer); break;
        case "indexBuffer": pass.setIndexBuffer(step.buffer, step.fmt); break;
        case "setViewport":
          pass.setViewport(step.x, step.y, step.w, step.h, step.minDepth, step.maxDepth);
          break;
        case "setScissor": pass.setScissorRect(step.x, step.y, step.w, step.h); break;
        case "draw":
          pass.draw(
            Math.max(0, step.vertexCount | 0), Math.max(1, step.instanceCount | 0),
            step.firstVertex | 0, step.firstInstance | 0,
          );
          break;
        case "drawIndexed":
          pass.drawIndexed(
            Math.max(0, step.indexCount | 0), Math.max(1, step.instanceCount | 0),
            step.firstIndex | 0, step.baseVertex | 0, step.firstInstance | 0,
          );
          break;
        case "drawIndirect": pass.drawIndirect(step.buffer, step.offset); break;
        case "drawIndexedIndirect": pass.drawIndexedIndirect(step.buffer, step.offset); break;
      }
    }
    pass.end();
    return touched;
  }

  /// Total ring space this frame's records will ask for. `setPipeline` is tracked because the
  /// block size is the pipeline's; a `setUniforms` naming an unknown one is left for the replay to
  /// reject.
  function uniformBytesNeeded(records, dev) {
    let total = 0;
    let rp = null;
    for (const rec of records) {
      if (rec.op === "setPipeline") rp = renderPipelines.get(rec.id);
      else if (rec.op === "setUniforms" && rp && (rp.uniformSize | 0) > 0) {
        total += uniformStride(rp.uniformSize | 0, dev);
      }
    }
    return total;
  }

  async function submitStream(stream) {
    const bytes = toU8(stream);
    if (bytes.byteLength === 0) return 0;
    const records = decodeStream(bytes);
    const dev = await ensureDevice();
    // Every draw gets its own slice of the ring, so the frame's whole requirement is reserved
    // before the first bind group pins the buffer. A reservation that outgrows the ring replaces
    // it, stranding every cached group on the buffer it was built against.
    if (beginUniformFrame(dev, uniformBytesNeeded(records, dev))) renderBgCache.clear();

    const encoder = dev.createCommandEncoder();
    const touched = [];
    let i = 0;
    while (i < records.length) {
      if (records[i].op !== "beginPass") {
        throw new Error(`command '${records[i].op}' outside of a render pass`);
      }
      const desc = records[i++].desc;
      const body = [];
      let closed = false;
      while (i < records.length) {
        const rec = records[i++];
        if (rec.op === "endPass") { closed = true; break; }
        body.push(rec);
      }
      if (!closed) throw new Error("render pass was never ended");
      touched.push(...await replayPass(dev, encoder, desc, body));
    }
    dev.queue.submit([encoder.finish()]);
    await dev.queue.onSubmittedWorkDone();
    // Render targets now hold fresh GPU content, so their CPU mirrors are stale.
    for (const t of touched) t.cpu = null;
    return 0;
  }

  const host = {
    __attachGpuAbi: attachFromAbi,

    gpuIsAvailable: () => !!(globalThis.navigator && globalThis.navigator.gpu),
    gpuReady: () => device != null && !deviceLost,
    gpuCheck: () => {
      if (deviceLost) return ERR_DEVICE_LOST;
      if (!device) return ERR_UNAVAILABLE;
      return 0;
    },
    // All-zero before `gpuTryInit`, so callers see "nothing available" rather than an optimistic
    // answer drawn from an adapter no device was ever requested from.
    gpuCapabilities: () => {
      const out = new Uint8Array(CAPS_BLOB_LEN);
      if (!device) return out;
      const has = (f) => !!device.features?.has(f);
      let flags = 0;
      if (has("shader-f16")) flags |= CAP_SHADER_FLOAT16;
      if (has("subgroups")) flags |= CAP_SUBGROUP;
      if (has("texture-compression-bc")) flags |= CAP_TEXTURE_COMPRESSION_BC;
      if (has("texture-compression-etc2")) flags |= CAP_TEXTURE_COMPRESSION_ETC2;
      if (has("texture-compression-astc")) flags |= CAP_TEXTURE_COMPRESSION_ASTC;
      if (has("depth32float-stencil8")) flags |= CAP_DEPTH32_FLOAT_STENCIL8;
      if (has("float32-filterable")) flags |= CAP_FLOAT32_FILTERABLE;
      if (has("timestamp-query")) flags |= CAP_TIMESTAMP_QUERY;
      if (has("timestamp-query-inside-encoders")) flags |= CAP_TIMESTAMP_QUERY_INSIDE_ENCODERS;
      // No subgroup-barrier or cooperative-matrix bit: WebGPU exposes no counterpart to the native
      // `SUBGROUP_BARRIER` feature, and subgroup-matrix tiles remain a pre-standard extension.
      const lim = device.limits || {};
      const u32 = (v) => (Number.isFinite(v) ? v >>> 0 : 0);
      const u64 = (v) => (Number.isFinite(v) && v > 0 ? BigInt(Math.floor(v)) : 0n);
      const dv = new DataView(out.buffer);
      dv.setUint32(0, flags >>> 0, true);
      dv.setUint32(4, 0, true);
      dv.setBigUint64(8, u64(lim.maxBufferSize), true);
      dv.setBigUint64(16, u64(lim.maxStorageBufferBindingSize), true);
      dv.setUint32(24, u32(lim.maxComputeWorkgroupStorageSize), true);
      dv.setUint32(28, u32(lim.maxComputeInvocationsPerWorkgroup), true);
      dv.setUint32(32, u32(lim.maxComputeWorkgroupSizeX), true);
      dv.setUint32(36, u32(lim.maxComputeWorkgroupSizeY), true);
      dv.setUint32(40, u32(lim.maxComputeWorkgroupSizeZ), true);
      dv.setUint32(44, u32(lim.maxComputeWorkgroupsPerDimension), true);
      // Subgroup width lives on the adapter info in the subgroups proposal, and on the device
      // limits in earlier drafts; zero when the host reports it in neither place.
      const info = gpuAdapter?.info || {};
      dv.setUint32(48, u32(info.subgroupMinSize ?? lim.minSubgroupSize), true);
      dv.setUint32(52, u32(info.subgroupMaxSize ?? lim.maxSubgroupSize), true);
      return out;
    },
    gpuLastError: () => {
      const msg = lastError;
      lastError = "";
      return msg;
    },
    gpuTryInit: async (power) => {
      try {
        if (deviceLost) {
          deviceLost = false;
          lastError = "";
        }
        const next = power | 0;
        if (next !== powerCode && !device) {
          devicePromise = null;
          gpuAdapter = null;
        }
        powerCode = next;
        await ensureDevice();
        return 0;
      } catch (e) {
        console.error("Dream Gpu.try_init:", e);
        return classifyErr(e);
      }
    },
    gpuFrame: () =>
      new Promise((resolve) => {
        if (typeof requestAnimationFrame === "function") {
          requestAnimationFrame(() => resolve());
        } else {
          setTimeout(resolve, 16);
        }
      }),
    gpuTimestamp: async () => {
      if (typeof performance !== "undefined" && performance.now) {
        return BigInt(Math.floor(performance.now() * 1e6));
      }
      return BigInt(Date.now()) * 1000000n;
    },

    gpuBufferAllocBytes: (n) => {
      const id = nextId++;
      buffers.set(id, { gpuBuffer: null, nbytes: Math.max(0, n | 0), cpu: null, usage: 0 });
      return id;
    },
    gpuBufferAllocVertexBytes: (n) => {
      const id = nextId++;
      buffers.set(id, {
        gpuBuffer: null,
        nbytes: Math.max(0, n | 0),
        cpu: null,
        usage: 0,
        vertex: true,
      });
      return id;
    },
    gpuBufferWriteBytes: (id, data) => {
      const b = buffers.get(id);
      if (!b) throw new Error(`unknown GpuBuffer ${id}`);
      const arr = toU8(data);
      b.cpu = arr;
      b.nbytes = arr.byteLength;
      b.gpuBuffer = null;
    },
    gpuBufferWriteBytesAt: (id, byteOffset, data) => {
      const b = buffers.get(id);
      if (!b) throw new Error(`unknown GpuBuffer ${id}`);
      const arr = toU8(data);
      const off = Math.max(0, byteOffset | 0);
      if (!(b.cpu instanceof Uint8Array) || b.cpu.byteLength < b.nbytes) {
        b.cpu = new Uint8Array(Math.max(b.nbytes, off + arr.byteLength));
      }
      if (off + arr.byteLength > b.cpu.byteLength) {
        const grown = new Uint8Array(off + arr.byteLength);
        grown.set(b.cpu);
        b.cpu = grown;
      }
      b.cpu.set(arr, off);
      b.nbytes = Math.max(b.nbytes, off + arr.byteLength);
      b.gpuBuffer = null;
    },
    gpuBufferReadBytes: async (id, n) => host.gpuBufferReadBytesAt(id, 0, n),
    gpuBufferReadBytesAt: async (id, byteOffset, n) => {
      const b = buffers.get(id);
      if (!b) throw new Error(`unknown GpuBuffer ${id}`);
      const nbytes = Math.max(0, n | 0);
      const off = Math.max(0, byteOffset | 0);
      if (b.gpuBuffer) {
        const dev = await ensureDevice();
        await syncBufferToCpu(dev, b);
      }
      if (!(b.cpu instanceof Uint8Array) && !b.cpu) {
        return Array(nbytes).fill(0);
      }
      const src = b.cpu instanceof Uint8Array ? b.cpu : new Uint8Array(b.cpu.buffer || []);
      const slice = src.slice(off, off + nbytes);
      if (slice.length >= nbytes) return Array.from(slice);
      const out = Array(nbytes).fill(0);
      for (let i = 0; i < slice.length; i++) out[i] = slice[i];
      return out;
    },
    gpuBufferDestroy: (id) => {
      const b = buffers.get(id);
      if (!b) return;
      buffers.delete(id);
      if (b.gpuBuffer) {
        try { b.gpuBuffer.destroy(); } catch (_) {}
      }
    },
    gpuBufferCopy: (srcId, dstId, srcOffset, dstOffset, size) => {
      const src = buffers.get(srcId);
      const dst = buffers.get(dstId);
      if (!src || !dst) throw new Error("gpuBufferCopy: bad buffer id");
      const n = Math.max(0, size | 0);
      const so = Math.max(0, srcOffset | 0);
      const doff = Math.max(0, dstOffset | 0);
      if (!device) {
        if (!(src.cpu instanceof Uint8Array)) src.cpu = new Uint8Array(Math.max(src.nbytes, so + n));
        if (!(dst.cpu instanceof Uint8Array) || dst.cpu.byteLength < doff + n) {
          const grown = new Uint8Array(Math.max(dst.nbytes, doff + n));
          if (dst.cpu) grown.set(dst.cpu);
          dst.cpu = grown;
          dst.nbytes = grown.byteLength;
        }
        dst.cpu.set(src.cpu.subarray(so, so + n), doff);
        return;
      }
      const need =
        GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST | GPUBufferUsage.COPY_SRC | GPUBufferUsage.INDIRECT;
      const ensureSync = (b) => {
        if (!b.gpuBuffer) {
          b.gpuBuffer = device.createBuffer({ size: Math.max(4, b.nbytes), usage: need });
          b.usage = need;
          if (b.cpu) {
            const bytes = b.cpu instanceof Uint8Array
              ? b.cpu
              : new Uint8Array(b.cpu.buffer, b.cpu.byteOffset, b.cpu.byteLength);
            device.queue.writeBuffer(b.gpuBuffer, 0, bytes);
          }
        }
        return b.gpuBuffer;
      };
      const sbuf = ensureSync(src);
      const dbuf = ensureSync(dst);
      const encoder = device.createCommandEncoder();
      encoder.copyBufferToBuffer(sbuf, so, dbuf, doff, n);
      device.queue.submit([encoder.finish()]);
      dst.cpu = null;
    },

    gpuDispatch: async (kernel, bufferIds, textureIds, samplerIds, ex, ey, ez, uniforms) => {
      try {
        return await runDispatch(
          kernel, bufferIds, textureIds, samplerIds, ex, ey, ez, uniforms,
        );
      } catch (e) {
        console.error("Dream gpuDispatch:", e);
        return classifyErr(e);
      }
    },

    gpuDispatchIndirect: async (
      kernel, bufferIds, textureIds, samplerIds, indirectId, indirectOffset,
    ) => {
      try {
        return await runDispatchIndirect(
          kernel, bufferIds, textureIds, samplerIds, indirectId, indirectOffset,
        );
      } catch (e) {
        console.error("Dream gpuDispatchIndirect:", e);
        return classifyErr(e);
      }
    },

    gpuShaderFromWgsl: (source, entry) => {
      const id = nextId++;
      shaders.set(id, { source: String(source), entry: String(entry) });
      return id;
    },
    gpuDispatchShader: async (shaderId, bufferIds, wx, wy, wz) => {
      const s = shaders.get(shaderId);
      if (!s) return ERR_OTHER;
      const prev = wgslSource;
      const prevAbi = gpuAbi;
      wgslSource = null;
      gpuAbi = {
        kernels: [{
          name: `__raw_${shaderId}`,
          entry: s.entry,
          workgroup: [wx || 64, wy || 1, wz || 1],
          bindings: (bufferIds || []).map((_, i) => ({
            name: `b${i}`, binding: i, kind: "storage", type: "f32", read_write: true, atomic: false,
          })),
        }],
      };
      const inline = s.source;
      const oldLoad = loadWgslText;
      loadWgslText = async () => inline;
      try {
        return await host.gpuDispatch(
          `__raw_${shaderId}`, bufferIds, [], [], wx || 1, wy || 1, wz || 1, [],
        );
      } finally {
        loadWgslText = oldLoad;
        wgslSource = prev;
        gpuAbi = prevAbi;
      }
    },

    gpuSamplerCreate: (
      magFilter, minFilter, mipFilter,
      addressU, addressV, addressW,
      lodMin, lodMax, compare, maxAnisotropy,
    ) => {
      const id = nextId++;
      samplers.set(id, {
        sampler: null,
        magFilter: FILTER_MODES[magFilter | 0] || "nearest",
        minFilter: FILTER_MODES[minFilter | 0] || "nearest",
        mipmapFilter: FILTER_MODES[mipFilter | 0] || "nearest",
        addressModeU: ADDRESS_MODES[addressU | 0] || "clamp-to-edge",
        addressModeV: ADDRESS_MODES[addressV | 0] || "clamp-to-edge",
        addressModeW: ADDRESS_MODES[addressW | 0] || "clamp-to-edge",
        lodMinClamp: lodMin,
        lodMaxClamp: Math.max(lodMin, lodMax),
        compare: compare >= 0 ? COMPARE_FUNCTIONS[compare | 0] : undefined,
        maxAnisotropy: Math.min(16, Math.max(1, maxAnisotropy | 0)),
      });
      return id;
    },

    gpuTextureCreate: (
      format, dimension, width, height,
      depthOrLayers, mipLevels, sampleCount, storageAccess, viewDimension,
    ) => {
      const spec = TEXTURE_FORMATS[format | 0];
      if (!spec) return -classifyErr(new Error(`unknown texture format ${format}`));
      const dim = TEXTURE_DIMENSIONS[dimension | 0];
      const view = VIEW_DIMENSIONS[viewDimension | 0];
      if (!dim || !view) {
        return -classifyErr(new Error(`unknown texture dimension ${dimension}/${viewDimension}`));
      }
      if (storageAccess >= 0 && !spec.storage) {
        return -classifyErr(
          new Error(`validation: texture format ${spec.name} cannot be a storage texture`),
        );
      }
      const w = Math.max(1, width | 0);
      const h = Math.max(1, height | 0);
      const layers = Math.max(1, depthOrLayers | 0);
      const mips = Math.max(1, mipLevels | 0);
      const samples = Math.max(1, sampleCount | 0);
      const shapeErr = validateTextureShape(spec, dim, view, w, h, layers, mips, samples);
      if (shapeErr) return -classifyErr(new Error(shapeErr));
      const id = nextId++;
      textures.set(id, {
        texture: null,
        width: w,
        height: h,
        // Only linearly copyable color formats get a CPU mirror; compressed and depth textures
        // are GPU-only, so writes and reads through it are rejected instead.
        cpu: spec.bytesPerTexel && !spec.depth
          ? new Uint8Array(w * h * layers * spec.bytesPerTexel)
          : null,
        storage: storageAccess >= 0,
        format: spec.name,
        spec,
        depth: spec.depth,
        dimension: dim,
        viewDimension: view,
        depth_or_layers: layers,
        mip_levels: mips,
        sample_count: samples,
      });
      return id;
    },
    gpuTextureFromImageBytes: async (pixels) => {
      try {
        const src = toU8(pixels);
        if (!src || src.length === 0) {
          return [-classifyErr(new Error("validation: image bytes are empty"))];
        }
        if (typeof createImageBitmap !== "function") {
          return [-classifyErr(new Error("unsupported: createImageBitmap is not available"))];
        }
        const blob = new Blob([src]);
        const bitmap = await createImageBitmap(blob);
        const w = bitmap.width | 0;
        const h = bitmap.height | 0;
        if (w <= 0 || h <= 0) {
          bitmap.close?.();
          return [-classifyErr(new Error("validation: decoded image has zero size"))];
        }
        if (w > 8192 || h > 8192) {
          bitmap.close?.();
          return [-classifyErr(new Error(`unsupported: image ${w}x${h} exceeds 8192 on an edge`))];
        }
        let rgba = null;
        if (typeof OffscreenCanvas !== "undefined") {
          const canvas = new OffscreenCanvas(w, h);
          const ctx = canvas.getContext("2d");
          if (ctx) {
            ctx.drawImage(bitmap, 0, 0);
            rgba = ctx.getImageData(0, 0, w, h).data;
          }
        }
        const id = nextId++;
        textures.set(id, {
          texture: null,
          width: w,
          height: h,
          cpu: rgba ? new Uint8Array(rgba) : new Uint8Array(w * h * 4),
          storage: false,
          format: "rgba8unorm",
          spec: TEXTURE_FORMATS[2],
          depth: false,
          dimension: "2d",
          viewDimension: "2d",
          depth_or_layers: 1,
          mip_levels: 1,
          sample_count: 1,
        });
        const t = textures.get(id);
        const dev = await ensureDevice();
        await ensureTexture(dev, t, false);
        if (dev.queue.copyExternalImageToTexture) {
          dev.queue.copyExternalImageToTexture(
            { source: bitmap },
            { texture: t.texture },
            [w, h],
          );
        } else if (t.cpu) {
          dev.queue.writeTexture(
            { texture: t.texture },
            t.cpu,
            { bytesPerRow: w * 4 },
            [w, h],
          );
        }
        bitmap.close?.();
        return [id, w, h];
      } catch (e) {
        const msg = String(e && e.message ? e.message : e);
        if (!/unsupported|validation/i.test(msg)) {
          lastError = `unsupported: could not decode image (${msg})`;
          return [-ERR_UNSUPPORTED];
        }
        return [-classifyErr(e)];
      }
    },
    gpuTextureWriteRgba: async (id, pixels, x, y, w, h) => {
      try {
        const t = textures.get(id);
        if (!t) throw new Error(`unknown GpuTexture ${id}`);
        const px = Math.max(0, x | 0);
        const py = Math.max(0, y | 0);
        const pw = Math.max(0, w | 0);
        const ph = Math.max(0, h | 0);
        const src = toU8(pixels);
        for (let row = 0; row < ph; row++) {
          const dstOff = ((py + row) * t.width + px) * 4;
          const srcOff = row * pw * 4;
          t.cpu.set(src.subarray(srcOff, srcOff + pw * 4), dstOff);
        }
        // Base level changed — drop any mip chain so the next ensure recreates single-level.
        if ((t.mip_levels | 0) > 1) {
          t.mip_levels = 1;
          if (t.texture) {
            try { t.texture.destroy(); } catch (_) {}
            t.texture = null;
            t.view = null;
          }
        }
        const dev = await ensureDevice();
        await ensureTexture(dev, t, t.storage);
        dev.queue.writeTexture(
          { texture: t.texture, origin: [px, py] },
          src,
          { bytesPerRow: pw * 4 },
          [pw, ph],
        );
        return 0;
      } catch (e) {
        console.error("Dream gpuTextureWriteRgba:", e);
        return classifyErr(e);
      }
    },
    gpuTextureReadRgba: async (id) => {
      const t = textures.get(id);
      if (!t) throw new Error(`unknown GpuTexture ${id}`);
      if (t.texture) {
        const dev = await ensureDevice();
        const bytesPerRow = Math.ceil((t.width * 4) / 256) * 256;
        const staging = dev.createBuffer({
          size: bytesPerRow * t.height,
          usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
        });
        const encoder = dev.createCommandEncoder();
        encoder.copyTextureToBuffer(
          { texture: t.texture },
          { buffer: staging, bytesPerRow },
          [t.width, t.height],
        );
        dev.queue.submit([encoder.finish()]);
        await staging.mapAsync(GPUMapMode.READ);
        const mapped = new Uint8Array(staging.getMappedRange());
        const out = new Uint8Array(t.width * t.height * 4);
        for (let row = 0; row < t.height; row++) {
          out.set(
            mapped.subarray(row * bytesPerRow, row * bytesPerRow + t.width * 4),
            row * t.width * 4,
          );
        }
        staging.unmap();
        staging.destroy();
        t.cpu = out;
      }
      return Array.from(t.cpu);
    },
    gpuTextureCopyFromBuffer: (texId, bufId, byteOffset, x, y, w, h) => {
      const t = textures.get(texId);
      const b = buffers.get(bufId);
      if (!t || !b) throw new Error("texture_copy_from_buffer: bad id");
      if (!device) {
        // CPU staging: copy bytes into texture CPU shadow.
        const off = Math.max(0, byteOffset | 0);
        const src = b.cpu instanceof Uint8Array ? b.cpu : new Uint8Array(b.nbytes);
        const pw = w | 0;
        const ph = h | 0;
        const px = x | 0;
        const py = y | 0;
        if (!(t.cpu instanceof Uint8Array)) t.cpu = new Uint8Array(t.width * t.height * 4);
        for (let row = 0; row < ph; row++) {
          const dstOff = ((py + row) * t.width + px) * 4;
          const srcOff = off + row * pw * 4;
          t.cpu.set(src.subarray(srcOff, srcOff + pw * 4), dstOff);
        }
        return;
      }
      const need =
        GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST | GPUBufferUsage.COPY_SRC | GPUBufferUsage.INDIRECT;
      if (!b.gpuBuffer) {
        b.gpuBuffer = device.createBuffer({ size: Math.max(4, b.nbytes), usage: need });
        if (b.cpu) device.queue.writeBuffer(b.gpuBuffer, 0, b.cpu);
      }
      const usage =
        GPUTextureUsage.TEXTURE_BINDING |
        GPUTextureUsage.STORAGE_BINDING |
        GPUTextureUsage.COPY_DST |
        GPUTextureUsage.COPY_SRC;
      if (!t.texture) {
        t.texture = device.createTexture({ size: [t.width, t.height], format: "rgba8unorm", usage });
      }
      const encoder = device.createCommandEncoder();
      encoder.copyBufferToTexture(
        { buffer: b.gpuBuffer, offset: Math.max(0, byteOffset | 0), bytesPerRow: (w | 0) * 4 },
        { texture: t.texture, origin: [x | 0, y | 0] },
        [w | 0, h | 0],
      );
      device.queue.submit([encoder.finish()]);
      t.cpu = null;
    },
    gpuTextureCopyToBuffer: (texId, bufId, byteOffset, x, y, w, h) => {
      const t = textures.get(texId);
      const b = buffers.get(bufId);
      if (!t || !b) throw new Error("texture_copy_to_buffer: bad id");
      if (!device) {
        const off = Math.max(0, byteOffset | 0);
        const pw = w | 0;
        const ph = h | 0;
        const px = x | 0;
        const py = y | 0;
        if (!(t.cpu instanceof Uint8Array)) t.cpu = new Uint8Array(t.width * t.height * 4);
        if (!(b.cpu instanceof Uint8Array) || b.cpu.byteLength < off + pw * ph * 4) {
          const grown = new Uint8Array(Math.max(b.nbytes, off + pw * ph * 4));
          if (b.cpu) grown.set(b.cpu);
          b.cpu = grown;
          b.nbytes = grown.byteLength;
        }
        for (let row = 0; row < ph; row++) {
          const srcOff = ((py + row) * t.width + px) * 4;
          const dstOff = off + row * pw * 4;
          b.cpu.set(t.cpu.subarray(srcOff, srcOff + pw * 4), dstOff);
        }
        return;
      }
      const need =
        GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST | GPUBufferUsage.COPY_SRC | GPUBufferUsage.INDIRECT;
      if (!b.gpuBuffer) {
        b.gpuBuffer = device.createBuffer({ size: Math.max(4, b.nbytes), usage: need });
        if (b.cpu) device.queue.writeBuffer(b.gpuBuffer, 0, b.cpu);
      }
      const usage =
        GPUTextureUsage.TEXTURE_BINDING |
        GPUTextureUsage.STORAGE_BINDING |
        GPUTextureUsage.COPY_DST |
        GPUTextureUsage.COPY_SRC;
      if (!t.texture) {
        t.texture = device.createTexture({ size: [t.width, t.height], format: "rgba8unorm", usage });
        if (t.cpu) {
          device.queue.writeTexture(
            { texture: t.texture }, t.cpu, { bytesPerRow: t.width * 4 }, [t.width, t.height],
          );
        }
      }
      const encoder = device.createCommandEncoder();
      encoder.copyTextureToBuffer(
        { texture: t.texture, origin: [x | 0, y | 0] },
        { buffer: b.gpuBuffer, offset: Math.max(0, byteOffset | 0), bytesPerRow: (w | 0) * 4 },
        [w | 0, h | 0],
      );
      device.queue.submit([encoder.finish()]);
      b.cpu = null;
    },

    gpuTextureDestroy: (id) => {
      const t = textures.get(id);
      if (!t) return;
      textures.delete(id);
      if (t.texture) {
        try { t.texture.destroy(); } catch (_) {}
      }
    },
    gpuSamplerDestroy: (id) => {
      samplers.delete(id);
    },
    gpuTextureCopy: (srcId, dstId, srcX, srcY, dstX, dstY, width, height) => {
      const src = textures.get(srcId);
      const dst = textures.get(dstId);
      if (!src || !dst) return;
      if (src.depth || dst.depth) return;
      if ((src.format || "rgba8unorm") !== (dst.format || "rgba8unorm")) return;
      const bpp = (src.format || "rgba8unorm") === "rgba16float" ? 8 : 4;
      const sx = Math.max(0, srcX | 0);
      const sy = Math.max(0, srcY | 0);
      const dx = Math.max(0, dstX | 0);
      const dy = Math.max(0, dstY | 0);
      let w = Math.max(0, width | 0);
      let h = Math.max(0, height | 0);
      w = Math.min(w, Math.max(0, src.width - sx), Math.max(0, dst.width - dx));
      h = Math.min(h, Math.max(0, src.height - sy), Math.max(0, dst.height - dy));
      if (w === 0 || h === 0) return;
      if (device && src.texture && dst.texture) {
        const encoder = device.createCommandEncoder();
        encoder.copyTextureToTexture(
          { texture: src.texture, origin: [sx, sy, 0] },
          { texture: dst.texture, origin: [dx, dy, 0] },
          [w, h, 1],
        );
        device.queue.submit([encoder.finish()]);
        dst.cpu = null;
        return;
      }
      // CPU shadow fallback.
      if (!(src.cpu instanceof Uint8Array)) return;
      if (!(dst.cpu instanceof Uint8Array)) {
        dst.cpu = new Uint8Array(dst.width * dst.height * bpp);
      }
      const strideSrc = src.width * bpp;
      const strideDst = dst.width * bpp;
      const rowBytes = w * bpp;
      for (let row = 0; row < h; row++) {
        const so = (sy + row) * strideSrc + sx * bpp;
        const doff = (dy + row) * strideDst + dx * bpp;
        dst.cpu.set(src.cpu.subarray(so, so + rowBytes), doff);
      }
    },
    gpuTextureGenerateMipmaps: (id) => {
      const t = textures.get(id);
      if (!t) return ERR_OTHER;
      if (t.depth) return ERR_VALIDATION;
      if ((t.format || "rgba8unorm") !== "rgba8unorm") return ERR_VALIDATION;
      const width = Math.max(1, t.width | 0);
      const height = Math.max(1, t.height | 0);
      const mipCount = Math.floor(Math.log2(Math.max(width, height))) + 1;
      // Compute CPU mip levels once via box filter. `ensureTexture` recreates the GPU texture
      // when `mip_levels` changes; we then upload each level into its own subresource.
      if (!(t.cpu instanceof Uint8Array) || t.cpu.byteLength < width * height * 4) {
        t.cpu = new Uint8Array(width * height * 4);
      }
      const levels = [t.cpu];
      let prev = t.cpu;
      let prevW = width;
      let prevH = height;
      for (let level = 1; level < mipCount; level++) {
        const nextW = Math.max(1, prevW >> 1);
        const nextH = Math.max(1, prevH >> 1);
        const next = new Uint8Array(nextW * nextH * 4);
        for (let y = 0; y < nextH; y++) {
          for (let x = 0; x < nextW; x++) {
            const sx0 = Math.min(x * 2, prevW - 1);
            const sx1 = Math.min(x * 2 + 1, prevW - 1);
            const sy0 = Math.min(y * 2, prevH - 1);
            const sy1 = Math.min(y * 2 + 1, prevH - 1);
            for (let c = 0; c < 4; c++) {
              const a = prev[(sy0 * prevW + sx0) * 4 + c];
              const b = prev[(sy0 * prevW + sx1) * 4 + c];
              const cc = prev[(sy1 * prevW + sx0) * 4 + c];
              const d = prev[(sy1 * prevW + sx1) * 4 + c];
              next[(y * nextW + x) * 4 + c] = ((a + b + cc + d + 2) >>> 2) & 0xff;
            }
          }
        }
        levels.push(next);
        prev = next;
        prevW = nextW;
        prevH = nextH;
      }
      t.mip_levels = mipCount;
      // Force ensureTexture to recreate with the new mip count; the caller will re-encode
      // the texture on next use (via runDispatch, buildRenderBindGroups, or ensureBlit).
      if (t.texture) {
        try { t.texture.destroy(); } catch (_) {}
        t.texture = null;
      }
      if (device) {
        const usage =
          GPUTextureUsage.TEXTURE_BINDING |
          GPUTextureUsage.STORAGE_BINDING |
          GPUTextureUsage.COPY_DST |
          GPUTextureUsage.COPY_SRC |
          GPUTextureUsage.RENDER_ATTACHMENT;
        t.texture = device.createTexture({
          size: [width, height, 1],
          format: "rgba8unorm",
          usage,
          mipLevelCount: mipCount,
        });
        let mipW = width;
        let mipH = height;
        for (let level = 0; level < mipCount; level++) {
          device.queue.writeTexture(
            { texture: t.texture, mipLevel: level },
            levels[level],
            { bytesPerRow: mipW * 4 },
            [mipW, mipH],
          );
          mipW = Math.max(1, mipW >> 1);
          mipH = Math.max(1, mipH >> 1);
        }
      }
      return 0;
    },
    gpuRenderPipelineDestroy: (id) => {
      const rp = renderPipelines.get(id);
      if (!rp) return;
      renderPipelines.delete(id);
      for (const key of renderBgCache.keys()) {
        if (key.split(":")[0] === String(id)) renderBgCache.delete(key);
      }
      for (const [bgId, bg] of bindGroups) {
        if (bg.pipelineId === id) bindGroups.delete(bgId);
      }
      // Remove any cache entries pointing at this pipeline so a subsequent create can rebuild.
      for (const [k, v] of renderPipelineCache) {
        if (v === id) renderPipelineCache.delete(k);
      }
    },
    gpuSurfaceDestroy: (id) => {
      const s = surfaces.get(id);
      if (!s) return;
      surfaces.delete(id);
      // WebGPU canvas contexts are unconfigured by dropping the surface record; the canvas
      // element itself is owned by the page and stays untouched. Reset the flag so a later
      // re-registration reconfigures cleanly.
      s.configured = false;
      s.context = null;
    },

    gpuSurfaceFromCanvas: (canvasId) => {
      return host.gpuSurfaceCreate(canvasId, 800, 600);
    },
    gpuSurfaceCreate: (title, width, height) => {
      if (typeof document === "undefined") return -1;
      const el =
        document.getElementById(String(title)) || document.querySelector("canvas");
      if (!el || typeof el.getContext !== "function") return -1;
      const id = nextId++;
      const w = Math.max(1, width | 0);
      const h = Math.max(1, height | 0);
      el.width = w;
      el.height = h;
      const surface = {
        canvas: el,
        context: null,
        width: w,
        height: h,
        clientWidth: w,
        clientHeight: h,
        maxPixelRatio: 1,
        pixelRatio: 1,
        scaleFactor: globalThis.devicePixelRatio || 1,
        pointerLocked: false,
        fullscreen: false,
        configured: false,
        lastTexture: null,
        input: makeInputState(),
        alphaMode: "opaque",
        colorSpace: "srgb",
        presentMode: 2,
      };
      surfaces.set(id, surface);
      attachSurfaceInput(surface);
      syncSurfaceClientSize(surface);
      return id;
    },
    gpuSurfaceConfigure: (id, width, height, presentMode, alphaMode, colorSpace, maxPixelRatio) => {
      const s = surfaces.get(id);
      if (!s) throw new Error(`unknown GpuSurface ${id}`);
      s.clientWidth = Math.max(1, width | 0);
      s.clientHeight = Math.max(1, height | 0);
      s.maxPixelRatio = Number(maxPixelRatio) > 1 ? Number(maxPixelRatio) : 1;
      s.canvas.style.width = `${s.clientWidth}px`;
      s.canvas.style.height = `${s.clientHeight}px`;
      s.presentMode = presentMode | 0;
      s.alphaMode = (alphaMode | 0) === 2 ? "premultiplied" : "opaque";
      s.colorSpace = (colorSpace | 0) === 1 ? "display-p3" : "srgb";
      s.configured = false;
      syncSurfaceClientSize(s);
    },
    gpuSurfacePresent: async (id) => {
      return surfaces.has(id) ? 0 : ERR_OTHER;
    },
    gpuSurfacePointer: (id) => {
      const s = surfaces.get(id);
      if (!s || !s.input) return new Uint8Array(32);
      return packPointer(s.input);
    },
    gpuSurfacePointers: (id) => {
      const s = surfaces.get(id);
      if (!s || !s.input) return new Uint8Array(4);
      return packPointers(s.input);
    },
    gpuSurfacePixelRatio: (id) => {
      const s = surfaces.get(id);
      return s ? Number(s.pixelRatio) || 1 : 1;
    },
    gpuSurfaceScaleFactor: (id) => {
      const s = surfaces.get(id);
      if (!s) return 1;
      return Number(s.scaleFactor) || globalThis.devicePixelRatio || 1;
    },
    gpuSurfaceRequestPointerLock: (id) => {
      const s = surfaces.get(id);
      if (!s || !s.canvas || typeof s.canvas.requestPointerLock !== "function") return;
      const p = s.canvas.requestPointerLock({ unadjustedMovement: true });
      if (p && typeof p.catch === "function") {
        p.catch(() => s.canvas.requestPointerLock());
      }
    },
    gpuSurfaceExitPointerLock: (id) => {
      const s = surfaces.get(id);
      if (!s) return;
      if (typeof document !== "undefined" && document.exitPointerLock) {
        document.exitPointerLock();
      }
      s.pointerLocked = false;
    },
    gpuSurfacePointerLocked: (id) => {
      const s = surfaces.get(id);
      if (!s) return false;
      if (typeof document !== "undefined") {
        s.pointerLocked = document.pointerLockElement === s.canvas;
      }
      return !!s.pointerLocked;
    },
    gpuSurfaceRequestFullscreen: (id) => {
      const s = surfaces.get(id);
      if (!s || !s.canvas) return;
      const el = s.canvas;
      const req = el.requestFullscreen || el.webkitRequestFullscreen;
      if (typeof req === "function") {
        try {
          const p = req.call(el);
          if (p && typeof p.catch === "function") p.catch(() => {});
        } catch (_) {}
      }
    },
    gpuSurfaceExitFullscreen: (id) => {
      const s = surfaces.get(id);
      if (!s) return;
      const exit = document.exitFullscreen || document.webkitExitFullscreen;
      if (typeof exit === "function") {
        try {
          const p = exit.call(document);
          if (p && typeof p.catch === "function") p.catch(() => {});
        } catch (_) {}
      }
      s.fullscreen = false;
    },
    gpuSurfaceFullscreen: (id) => {
      const s = surfaces.get(id);
      if (!s) return false;
      if (typeof document !== "undefined") {
        const fsEl = document.fullscreenElement || document.webkitFullscreenElement;
        s.fullscreen = fsEl === s.canvas;
      }
      return !!s.fullscreen;
    },
    gpuSurfaceMods: (id) => {
      const s = surfaces.get(id);
      if (!s || !s.input) return new Uint8Array(4);
      return packMods(s.input);
    },
    gpuSurfaceKeyDown: (id, code) => {
      const s = surfaces.get(id);
      if (!s || !s.input) return false;
      return s.input.keysDown.has(String(code));
    },
    gpuSurfaceGamepads: (id) => {
      const s = surfaces.get(id);
      if (!s || !s.input) return [];
      syncGamepads(s.input);
      return [...s.input.pads.keys()].sort((a, b) => a - b);
    },
    gpuSurfaceGamepadConnected: (id, pad) => {
      const s = surfaces.get(id);
      if (!s || !s.input) return false;
      syncGamepads(s.input);
      return s.input.pads.has(pad | 0);
    },
    gpuSurfaceGamepadButtonDown: (id, pad, button) => {
      const s = surfaces.get(id);
      if (!s || !s.input) return false;
      syncGamepads(s.input);
      const st = s.input.pads.get(pad | 0);
      return !!(st && st.buttons.has(button | 0));
    },
    gpuSurfaceGamepadAxis: (id, pad, axis) => {
      const s = surfaces.get(id);
      if (!s || !s.input) return 0;
      syncGamepads(s.input);
      const st = s.input.pads.get(pad | 0);
      if (!st) return 0;
      const a = axis | 0;
      return a >= 0 && a < st.axes.length ? st.axes[a] : 0;
    },
    gpuSurfaceFocused: (id) => {
      const s = surfaces.get(id);
      return !!(s && s.input && s.input.focused);
    },
    gpuSurfaceCloseRequested: (id) => {
      const s = surfaces.get(id);
      return !!(s && s.input && s.input.closeRequested);
    },
    gpuSurfacePollEvents: (id) => {
      const s = surfaces.get(id);
      if (!s || !s.input) return new Uint8Array(4);
      return packEvents(s.input);
    },
    gpuSurfaceWidth: (id) => {
      const s = surfaces.get(id);
      return s ? s.width | 0 : 0;
    },
    gpuSurfaceHeight: (id) => {
      const s = surfaces.get(id);
      return s ? s.height | 0 : 0;
    },
    gpuRenderBlit: async (surfaceId, textureId) => {
      try {
        const s = surfaces.get(surfaceId);
        const t = textures.get(textureId);
        if (!s || !t) throw new Error("blit: bad surface/texture id");
        const dev = await ensureDevice();
        await ensureBlit(dev);
        await ensureTexture(dev, t, false);
        if (!s.context) {
          s.context = s.canvas.getContext("webgpu");
          if (!s.context) throw new Error("canvas webgpu context unavailable");
        }
        if (!s.configured) {
          s.context.configure({
            device: dev,
            format: navigator.gpu.getPreferredCanvasFormat(),
            alphaMode: s.alphaMode || "opaque",
            colorSpace: s.colorSpace || "srgb",
          });
          s.configured = true;
        }
        const view = s.context.getCurrentTexture().createView();
        const bg = dev.createBindGroup({
          layout: blitBindLayout,
          entries: [
            { binding: 0, resource: blitSampler },
            { binding: 1, resource: t.texture.createView() },
          ],
        });
        const encoder = dev.createCommandEncoder();
        const pass = encoder.beginRenderPass({
          colorAttachments: [{
            view,
            clearValue: { r: 0, g: 0, b: 0, a: 1 },
            loadOp: "clear",
            storeOp: "store",
          }],
        });
        pass.setPipeline(blitPipeline);
        pass.setBindGroup(0, bg);
        pass.draw(3);
        pass.end();
        dev.queue.submit([encoder.finish()]);
        await dev.queue.onSubmittedWorkDone();
        return 0;
      } catch (e) {
        console.error("Dream gpuRenderBlit:", e);
        return classifyErr(e);
      }
    },

    gpuRenderPipelineCreate: async (vertexName, fragmentName) => {
      return await host.gpuRenderPipelineCreateEx(
        vertexName, fragmentName, 0, 0, 0, 0, 0, 0, 0, 1,
      );
    },

    gpuRenderPipelineCreateEx: async (
      vertexName, fragmentName,
      topology, cullMode, frontFace,
      depthEnabled, depthWrite, depthCompare,
      blendEnabled, sampleCount,
    ) => {
      try {
        const vsName = String(vertexName);
        const fsName = String(fragmentName);
        const cacheKey = [
          vsName, fsName, topology, cullMode, frontFace,
          depthEnabled, depthWrite, depthCompare, blendEnabled, sampleCount,
        ].join("\0");
        if (renderPipelineCache.has(cacheKey)) {
          return renderPipelineCache.get(cacheKey);
        }
        const shaders = (gpuAbi && gpuAbi.shaders) || [];
        const vsMeta = shaders.find((s) => s.name === vsName && s.stage === "vertex");
        const fsMeta = shaders.find((s) => s.name === fsName && s.stage === "fragment");
        if (!vsMeta) throw new Error(`unknown @vertex shader '${vsName}'`);
        if (!fsMeta) throw new Error(`unknown @fragment shader '${fsName}'`);
        const dev = await ensureDevice();
        const vsModule = dev.createShaderModule({ code: vsMeta.source || "" });
        const fsModule = dev.createShaderModule({ code: fsMeta.source || "" });
        const format = navigator.gpu.getPreferredCanvasFormat();
        const allBinds = [...(vsMeta.bindings || []), ...(fsMeta.bindings || [])];
        const groups = planGroups(allBinds);
        // Both stages fold their uniform parameters into one shared block, so a stage declaring
        // none reports 0 and the other stage's size is the pipeline's.
        const vsUniform = vsMeta.uniform_size | 0;
        const fsUniform = fsMeta.uniform_size | 0;
        if (vsUniform !== 0 && fsUniform !== 0 && vsUniform !== fsUniform) {
          throw new Error(
            `@vertex '${vs}' and @fragment '${fs}' declare different uniform blocks ` +
            `(${vsUniform} vs ${fsUniform} bytes); the stages of one pipeline share a single block`,
          );
        }
        const uniformSize = Math.max(vsUniform, fsUniform);
        const layouts = createGroupLayouts(
          dev,
          groups,
          GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          uniformSize,
        );
        const layout = layouts.length
          ? dev.createPipelineLayout({ bindGroupLayouts: layouts })
          : "auto";
        // One layout per `@vertex` struct parameter, in `set_vertex_buffer` slot order. A buffer
        // whose struct has no attributes contributes no slot, which keeps the array contiguous.
        const vertexBuffers = (vsMeta.vertex_buffers || [])
          .filter((b) => (b.stride | 0) > 0 && (b.attributes || []).length > 0)
          .map((b) => ({
            arrayStride: b.stride | 0,
            stepMode: b.step_mode === "instance" ? "instance" : "vertex",
            attributes: b.attributes.map((a) => ({
              shaderLocation: a.location | 0,
              offset: a.offset | 0,
              format: a.format || "float32x4",
            })),
          }));
        const topologies = [
          "triangle-list", "triangle-strip", "line-list", "line-strip", "point-list",
        ];
        const cullModes = ["none", "front", "back"];
        const frontFaces = ["ccw", "cw"];
        const compares = [
          "less", "less-equal", "greater", "greater-equal", "always", "never",
        ];
        const colorTargets = Math.max(1, (fsMeta.color_targets | 0) || 1);
        const targets = [];
        for (let i = 0; i < colorTargets; i++) {
          const target = { format };
          if (blendEnabled) {
            target.blend = {
              color: {
                srcFactor: "src-alpha",
                dstFactor: "one-minus-src-alpha",
                operation: "add",
              },
              alpha: {
                srcFactor: "one",
                dstFactor: "one-minus-src-alpha",
                operation: "add",
              },
            };
          }
          targets.push(target);
        }
        const desc = {
          layout,
          vertex: {
            module: vsModule,
            entryPoint: vsMeta.entry,
            buffers: vertexBuffers,
          },
          fragment: {
            module: fsModule,
            entryPoint: fsMeta.entry,
            targets,
          },
          primitive: {
            topology: topologies[topology | 0] || "triangle-list",
            cullMode: cullModes[cullMode | 0] || "none",
            frontFace: frontFaces[frontFace | 0] || "ccw",
          },
          multisample: { count: Math.max(1, sampleCount | 0) },
        };
        if (depthEnabled) {
          desc.depthStencil = {
            format: "depth24plus",
            depthWriteEnabled: !!depthWrite,
            depthCompare: compares[depthCompare | 0] || "less",
          };
        }
        const pipeline = await dev.createRenderPipelineAsync(desc);
        const id = nextId++;
        renderPipelines.set(id, {
          pipeline, vsMeta, fsMeta, layouts, groups, uniformSize,
          depthEnabled: !!depthEnabled,
          sampleCount: Math.max(1, sampleCount | 0),
        });
        renderPipelineCache.set(cacheKey, id);
        return id;
      } catch (e) {
        console.error("Dream gpuRenderPipelineCreateEx:", e);
        return -(classifyErr(e) || ERR_OTHER);
      }
    },

    gpuEncoderSubmit: async (stream) => {
      try {
        return await submitStream(stream);
      } catch (e) {
        console.error("Dream gpuEncoderSubmit:", e);
        return classifyErr(e);
      }
    },

    /// Pins a resource set for one `@group` of a pipeline so the host resolves it once instead of
    /// on every draw. Validated here so a bad material fails at load time.
    gpuBindGroupCreate: async (pipelineId, group, bufferIds, textureIds, samplerIds) => {
      try {
        const rp = renderPipelines.get(pipelineId);
        if (!rp) throw new Error(`unknown GpuRenderPipeline ${pipelineId}`);
        const plan = rp.groups.find((g) => g.group === (group | 0));
        if (!plan) {
          throw new Error(`validation: pipeline ${pipelineId} declares no @group(${group})`);
        }
        const bufs = toI32Arr(bufferIds);
        const texs = toI32Arr(textureIds);
        const samps = toI32Arr(samplerIds);
        const want = groupArity(plan.bindings);
        if (bufs.length < want.bufs || texs.length < want.texs || samps.length < want.samps) {
          throw new Error(
            `validation: @group(${group}) needs ${want.bufs} buffer(s), ${want.texs} texture(s), ` +
            `${want.samps} sampler(s); got ${bufs.length}, ${texs.length}, ${samps.length}`,
          );
        }
        const id = nextId++;
        bindGroups.set(id, {
          pipelineId,
          group: group | 0,
          bufferIds: Array.from(bufs),
          textureIds: Array.from(texs),
          samplerIds: Array.from(samps),
        });
        return id;
      } catch (e) {
        console.error("Dream gpuBindGroupCreate:", e);
        return -(classifyErr(e) || ERR_OTHER);
      }
    },

    gpuBindGroupDestroy: (id) => {
      bindGroups.delete(id);
      for (const key of renderBgCache.keys()) {
        if (key.split(":")[2] === String(id)) renderBgCache.delete(key);
      }
    },

    gpuPassBegin: (querySet, tsBegin, tsEnd) => {
      const id = nextId++;
      passes.set(id, {
        ops: [],
        querySet: querySet | 0,
        tsBegin: tsBegin | 0,
        tsEnd: tsEnd | 0,
      });
      return id;
    },
    gpuPassDispatch: (
      passId, kernel, bufferIds, textureIds, samplerIds, ex, ey, ez, uniforms,
    ) => {
      const p = passes.get(passId);
      if (!p) throw new Error(`unknown ComputePass ${passId}`);
      p.ops.push({
        kind: "dispatch",
        kernel: String(kernel),
        bufferIds: toI32Arr(bufferIds),
        textureIds: toI32Arr(textureIds),
        samplerIds: toI32Arr(samplerIds),
        ex: ex | 0,
        ey: ey | 0,
        ez: ez | 0,
        uniforms: toU8(uniforms),
      });
    },
    gpuPassDispatchIndirect: (
      passId, kernel, bufferIds, textureIds, samplerIds, indirectId, indirectOffset,
    ) => {
      const p = passes.get(passId);
      if (!p) throw new Error(`unknown ComputePass ${passId}`);
      p.ops.push({
        kind: "indirect",
        kernel: String(kernel),
        bufferIds: toI32Arr(bufferIds),
        textureIds: toI32Arr(textureIds),
        samplerIds: toI32Arr(samplerIds),
        indirectId: indirectId | 0,
        indirectOffset: indirectOffset | 0,
      });
    },
    gpuPassSubmit: async (passId) => {
      try {
        const p = passes.get(passId);
        if (!p) throw new Error(`unknown ComputePass ${passId}`);
        const ops = p.ops;
        const qsId = p.querySet | 0;
        const timed = qsId >= 0;
        passes.delete(passId);
        if (ops.length === 0 && !timed) return 0;
        const dev = await ensureDevice();
        let needed = 0;
        for (const op of ops) {
          const pipe = await getPipeline(dev, op.kernel);
          const size = pipe.meta.uniform_size | 0;
          if (size > 0) needed += uniformStride(size, dev);
        }
        beginUniformFrame(dev, needed);
        const encoder = dev.createCommandEncoder();
        const qs = timed ? querySets.get(qsId) : null;
        const tsWrites = qs && qs.querySet
          ? {
            querySet: qs.querySet,
            beginningOfPassWriteIndex: p.tsBegin >= 0 ? p.tsBegin | 0 : undefined,
            endingOfPassWriteIndex: p.tsEnd >= 0 ? p.tsEnd | 0 : undefined,
          }
          : undefined;
        const cpass = encoder.beginComputePass(tsWrites ? { timestampWrites: tsWrites } : {});
        for (const op of ops) {
          const pipe = await getPipeline(dev, op.kernel);
          if (op.kind === "dispatch") {
            const bg = await buildBindGroup(
              dev, pipe, op.bufferIds, op.textureIds, op.samplerIds, op.uniforms,
            );
            encodeDispatchInto(cpass, pipe, bg, op.ex, op.ey, op.ez);
          } else {
            const bg = await buildBindGroup(
              dev, pipe, op.bufferIds, op.textureIds, op.samplerIds, [],
            );
            await encodeDispatchIndirectInto(
              dev, cpass, pipe, bg, op.indirectId, op.indirectOffset,
            );
          }
        }
        cpass.end();
        if (qs && qs.querySet && qs.resolve && qs.readback) {
          encoder.resolveQuerySet(qs.querySet, 0, qs.count, qs.resolve, 0);
          encoder.copyBufferToBuffer(qs.resolve, 0, qs.readback, 0, qs.count * 8);
        }
        dev.queue.submit([encoder.finish()]);
        await dev.queue.onSubmittedWorkDone();
        return 0;
      } catch (e) {
        console.error("Dream gpuPassSubmit:", e);
        return classifyErr(e);
      }
    },
    gpuQuerySetCreateTimestamps: (count) => {
      if (!device || !device.features?.has("timestamp-query")) {
        lastError = "timestamp-query is not available on this device";
        return -ERR_UNSUPPORTED;
      }
      const n = Math.max(1, count | 0);
      const querySet = device.createQuerySet({ type: "timestamp", count: n });
      const bytes = n * 8;
      const resolve = device.createBuffer({
        size: bytes,
        usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC,
      });
      const readback = device.createBuffer({
        size: bytes,
        usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
      });
      const id = nextId++;
      querySets.set(id, { querySet, count: n, resolve, readback });
      return id;
    },
    gpuQuerySetDestroy: (id) => {
      const qs = querySets.get(id);
      if (!qs) return;
      qs.querySet?.destroy?.();
      qs.resolve?.destroy?.();
      qs.readback?.destroy?.();
      querySets.delete(id);
    },
    gpuQuerySetRead: async (id) => {
      const qs = querySets.get(id);
      if (!qs || !qs.readback) return [];
      await qs.readback.mapAsync(GPUMapMode.READ);
      const src = new BigUint64Array(qs.readback.getMappedRange().slice(0));
      qs.readback.unmap();
      const period = device?.queue?.getTimestampPeriod ? device.queue.getTimestampPeriod() : 1;
      const out = [];
      for (let i = 0; i < src.length; i++) {
        out.push(BigInt(Math.round(Number(src[i]) * period)));
      }
      return out;
    },
    gpuTimestampPeriod: () => {
      if (device?.queue?.getTimestampPeriod) return device.queue.getTimestampPeriod();
      return 1;
    },
  };

  return host;
}

export { makeGpuHost };
