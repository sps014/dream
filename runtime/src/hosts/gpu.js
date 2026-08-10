/**
 * WebGPU host for `system.gpu`. Buffers/textures/surfaces/samplers are tracked by integer id;
 * kernels come from the sibling `.wgsl` + `abi.gpu.kernels` metadata attached via `attachGpuAbi`.
 */
function makeGpuHost(getInstance) {
  const buffers = new Map(); // id -> { gpuBuffer, nbytes, cpu, usage }
  const shaders = new Map();
  const textures = new Map(); // id -> { texture, width, height, cpu, storage }
  const samplers = new Map(); // id -> { sampler, filter }
  const surfaces = new Map();
  const passes = new Map(); // id -> { ops: [...] }
  const pipelineCache = new Map();
  const renderPipelines = new Map(); // id -> { pipeline, vsMeta, fsMeta, layout }
  const renderPipelineCache = new Map(); // key -> id
  let nextId = 1;
  let devicePromise = null;
  let device = null;
  let gpuAbi = null;
  let wgslSource = null;
  let blitPipeline = null;
  let blitSampler = null;
  let blitBindLayout = null;

  let lastError = "";

  const ERR_UNAVAILABLE = 1;
  const ERR_TIMEOUT = 2;
  const ERR_VALIDATION = 3;
  const ERR_OTHER = 4;

  function classifyErr(err) {
    const msg = String(err && err.message ? err.message : err);
    lastError = msg;
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
        const adapter = await Promise.race([
          navigator.gpu.requestAdapter(),
          new Promise((_, reject) =>
            setTimeout(() => reject(new Error("WebGPU requestAdapter timed out")), 8000),
          ),
        ]);
        if (!adapter) throw new Error("no WebGPU adapter");
        device = await adapter.requestDevice();
        return device;
      })().catch((err) => {
        devicePromise = null;
        throw err;
      });
    }
    return devicePromise;
  }

  function attachFromAbi(abi, sourceHint) {
    gpuAbi = abi && abi.gpu ? abi.gpu : null;
    if (gpuAbi && typeof sourceHint === "string") {
      wgslSource = sourceHint.replace(/\.wasm$/, ".wgsl").replace(/\.abi\.json$/, ".wgsl");
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
      queue: [],
    };
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

  function packPointer(input) {
    const buf = new ArrayBuffer(32);
    const view = new DataView(buf);
    let o = 0;
    o = writeF32(view, o, input.x);
    o = writeF32(view, o, input.y);
    o = writeF32(view, o, input.dx);
    o = writeF32(view, o, input.dy);
    o = writeI32(view, o, input.buttons);
    o = writeI32(view, o, input.buttons !== 0 ? 1 : 0);
    o = writeI32(view, o, input.inside ? 1 : 0);
    writeI32(view, o, input.pointerId | 0);
    input.dx = 0;
    input.dy = 0;
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
      if (type === "down") {
        setPointerPos(input, x, y);
        input.buttons |= 1 << Math.max(0, Math.min(31, button));
        input.pointerId = pid;
        pushEvent(input, { tag: 0, x, y, button, pointerId: pid });
        try {
          canvas.setPointerCapture?.(pid);
        } catch (_) {}
      } else if (type === "up") {
        setPointerPos(input, x, y);
        input.buttons &= ~(1 << Math.max(0, Math.min(31, button)));
        pushEvent(input, { tag: 1, x, y, button, pointerId: pid });
      } else if (type === "move") {
        setPointerPos(input, x, y);
        input.pointerId = pid;
        pushEvent(input, { tag: 2, x, y, pointerId: pid });
      } else if (type === "enter") {
        input.inside = true;
        setPointerPos(input, x, y);
        input.pointerId = pid;
        pushEvent(input, { tag: 3, x, y, pointerId: pid });
      } else if (type === "leave") {
        input.inside = false;
        setPointerPos(input, x, y);
        pushEvent(input, { tag: 4, x, y, pointerId: pid });
      } else if (type === "cancel") {
        input.buttons = 0;
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
        const w = canvas.clientWidth | 0;
        const h = canvas.clientHeight | 0;
        if (w > 0 && h > 0) {
          pushEvent(input, { tag: 10, width: w, height: h });
        }
        const dpr = globalThis.devicePixelRatio || 1;
        pushEvent(input, { tag: 11, scale: dpr });
      });
      ro.observe(canvas);
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

  async function ensureTexture(dev, t, storage) {
    // Always request STORAGE_BINDING + TEXTURE_BINDING together. Compute paint writes
    // via storage; blit samples the same texture. Recreating when the flags differed
    // wiped GPU contents and produced a black canvas with no error.
    if (storage) t.storage = true;
    const format = t.format || "rgba8unorm";
    const usage =
      GPUTextureUsage.TEXTURE_BINDING |
      (format === "rgba8unorm" ? GPUTextureUsage.STORAGE_BINDING : 0) |
      GPUTextureUsage.COPY_DST |
      GPUTextureUsage.COPY_SRC |
      (t.depth ? GPUTextureUsage.RENDER_ATTACHMENT : 0);
    if (t.texture) return t.texture;
    const desc = {
      size: [t.width, t.height, t.depth_or_layers || 1],
      format,
      usage,
      mipLevelCount: Math.max(1, t.mip_levels | 0 || 1),
    };
    if (t.dimension === "cube") {
      desc.size = [t.width, t.height, 6];
    }
    t.texture = dev.createTexture(desc);
    if (t.cpu && !t.depth) {
      const bpp = format === "rgba16float" ? 8 : 4;
      dev.queue.writeTexture(
        { texture: t.texture },
        t.cpu,
        { bytesPerRow: t.width * bpp },
        [t.width, t.height],
      );
    }
    return t.texture;
  }

  async function ensureDepthTexture(dev, t) {
    t.depth = true;
    t.format = t.format || "depth24plus";
    const usage =
      GPUTextureUsage.TEXTURE_BINDING |
      GPUTextureUsage.RENDER_ATTACHMENT |
      GPUTextureUsage.COPY_SRC;
    if (t.texture) return t.texture;
    t.texture = dev.createTexture({
      size: [t.width, t.height],
      format: t.format,
      usage,
    });
    return t.texture;
  }

  async function ensureSampler(dev, s) {
    if (s.sampler) return s.sampler;
    const filter = s.filter === 1 ? "linear" : s.filter === 2 ? "linear" : "nearest";
    const address = ["clamp-to-edge", "repeat", "mirror-repeat"][s.address | 0] || "clamp-to-edge";
    s.sampler = dev.createSampler({
      magFilter: filter === "linear" ? "linear" : "nearest",
      minFilter: filter === "linear" ? "linear" : "nearest",
      mipmapFilter: s.mip_filter === 1 ? "linear" : "nearest",
      addressModeU: address,
      addressModeV: address,
      addressModeW: address,
      compare: s.compare || undefined,
    });
    return s.sampler;
  }

  function layoutEntryForBinding(b) {
    const base = { binding: b.binding, visibility: GPUShaderStage.COMPUTE };
    if (b.kind === "uniform") {
      return { ...base, buffer: { type: "uniform" } };
    }
    if (b.kind === "storage") {
      return {
        ...base,
        buffer: { type: b.read_write ? "storage" : "read-only-storage" },
      };
    }
    if (b.kind === "sampler") {
      return { ...base, sampler: { type: "filtering" } };
    }
    if (b.kind === "storage_texture") {
      return {
        ...base,
        storageTexture: { access: "write-only", format: "rgba8unorm", viewDimension: "2d" },
      };
    }
    // sampled texture
    return { ...base, texture: { sampleType: "float" } };
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
    const entries = (meta.bindings || []).map(layoutEntryForBinding);
    const seen = new Set();
    const unique = [];
    for (const e of entries) {
      if (seen.has(e.binding)) continue;
      seen.add(e.binding);
      unique.push(e);
    }
    const layout = dev.createBindGroupLayout({ entries: unique });
    const pipeline = await dev.createComputePipelineAsync({
      layout: dev.createPipelineLayout({ bindGroupLayouts: [layout] }),
      compute: { module, entryPoint: meta.entry },
    });
    pipe = { pipeline, layout, meta };
    pipelineCache.set(kernel, pipe);
    return pipe;
  }

  async function buildBindGroup(dev, pipe, bufferIds, textureIds, samplerIds, ex, ey, ez, uniforms) {
    const meta = pipe.meta;
    const bufIds = toI32Arr(bufferIds);
    const texIds = toI32Arr(textureIds);
    const sampIds = toI32Arr(samplerIds);
    const resources = [];
    const usedBindings = new Set();
    let storageIdx = 0;
    let textureIdx = 0;
    let samplerIdx = 0;
    const extra = toU8(uniforms);
    for (const bind of meta.bindings || []) {
      if (usedBindings.has(bind.binding)) continue;
      usedBindings.add(bind.binding);
      if (bind.kind === "uniform") {
        const ubuf = dev.createBuffer({
          size: 256,
          usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
        });
        const bytes = new Uint8Array(256);
        const i32 = new Int32Array(bytes.buffer);
        i32[0] = ex | 0;
        i32[1] = ey | 0;
        i32[2] = ez | 0;
        if (extra.byteLength > 0) {
          bytes.set(extra.subarray(0, Math.min(extra.byteLength, 256 - 12)), 12);
        }
        dev.queue.writeBuffer(ubuf, 0, bytes);
        resources.push({ binding: bind.binding, resource: { buffer: ubuf } });
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
      } else if (bind.kind === "storage_texture" || bind.kind === "texture") {
        const id = texIds[textureIdx++] | 0;
        const t = textures.get(id);
        if (!t) throw new Error(`missing texture id ${id} for binding ${bind.binding}`);
        const tex = await ensureTexture(dev, t, bind.kind === "storage_texture");
        resources.push({ binding: bind.binding, resource: tex.createView() });
      }
    }
    return dev.createBindGroup({ layout: pipe.layout, entries: resources });
  }

  function encodeDispatch(encoder, pipe, bg, ex, ey, ez) {
    const wg = pipe.meta.workgroup || [64, 1, 1];
    const gx = Math.max(1, Math.ceil((ex | 0) / (wg[0] || 64)));
    const gy = Math.max(1, Math.ceil((ey | 0) / (wg[1] || 1)));
    const gz = Math.max(1, Math.ceil((ez | 0) / (wg[2] || 1)));
    const pass = encoder.beginComputePass();
    pass.setPipeline(pipe.pipeline);
    pass.setBindGroup(0, bg);
    pass.dispatchWorkgroups(gx, gy, gz);
    pass.end();
  }

  async function encodeDispatchIndirect(dev, encoder, pipe, bg, indirectId, offset) {
    const b = buffers.get(indirectId);
    if (!b) throw new Error(`missing indirect buffer ${indirectId}`);
    const gpuBuf = await ensureGpuBuffer(dev, b);
    const pass = encoder.beginComputePass();
    pass.setPipeline(pipe.pipeline);
    pass.setBindGroup(0, bg);
    pass.dispatchWorkgroupsIndirect(gpuBuf, Math.max(0, offset | 0));
    pass.end();
  }

  async function runDispatch(kernel, bufferIds, textureIds, samplerIds, ex, ey, ez, uniforms) {
    const dev = await ensureDevice();
    const pipe = await getPipeline(dev, kernel);
    const bg = await buildBindGroup(
      dev, pipe, bufferIds, textureIds, samplerIds, ex, ey, ez, uniforms,
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
    const bg = await buildBindGroup(
      dev, pipe, bufferIds, textureIds, samplerIds, 1, 1, 1, [],
    );
    const encoder = dev.createCommandEncoder();
    await encodeDispatchIndirect(dev, encoder, pipe, bg, indirectId, indirectOffset);
    dev.queue.submit([encoder.finish()]);
    await dev.queue.onSubmittedWorkDone();
    return 0;
  }

  async function buildRenderBindGroup(dev, rp, uniforms) {
    const binds = [...(rp.vsMeta.bindings || []), ...(rp.fsMeta.bindings || [])];
    if (binds.length === 0) return null;
    const used = new Set();
    const entries = [];
    const extra = toU8(uniforms);
    for (const bind of binds) {
      if (used.has(bind.binding)) continue;
      used.add(bind.binding);
      if (bind.kind === "uniform") {
        const ubuf = dev.createBuffer({
          size: 256,
          usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
        });
        const bytes = new Uint8Array(256);
        if (extra.byteLength > 0) {
          bytes.set(extra.subarray(0, Math.min(extra.byteLength, 256)), 0);
        }
        dev.queue.writeBuffer(ubuf, 0, bytes);
        entries.push({ binding: bind.binding, resource: { buffer: ubuf } });
      }
      // textures/samplers for render draws can be added later via dedicated APIs
    }
    if (entries.length === 0) return null;
    return dev.createBindGroup({
      layout: rp.pipeline.getBindGroupLayout(0),
      entries,
    });
  }

  const host = {
    __attachGpuAbi: attachFromAbi,

    gpuIsAvailable: () => !!(globalThis.navigator && globalThis.navigator.gpu),
    gpuReady: () => device != null,
    gpuLastError: () => {
      const msg = lastError;
      lastError = "";
      return msg;
    },
    gpuTryInit: async () => {
      try {
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

    gpuSamplerCreate: (filter) => {
      const id = nextId++;
      samplers.set(id, { sampler: null, filter: filter | 0, address: 0, mip_filter: 0 });
      return id;
    },

    gpuSamplerCreateEx: (filter, address, mipFilter) => {
      const id = nextId++;
      samplers.set(id, {
        sampler: null,
        filter: filter | 0,
        address: address | 0,
        mip_filter: mipFilter | 0,
      });
      return id;
    },

    gpuTextureCreateRgba8: (width, height) => {
      const id = nextId++;
      const w = Math.max(1, width | 0);
      const h = Math.max(1, height | 0);
      textures.set(id, {
        texture: null, width: w, height: h, cpu: new Uint8Array(w * h * 4),
        storage: false, format: "rgba8unorm", mip_levels: 1,
      });
      return id;
    },

    gpuTextureCreateDepth: (width, height) => {
      const id = nextId++;
      const w = Math.max(1, width | 0);
      const h = Math.max(1, height | 0);
      textures.set(id, {
        texture: null, width: w, height: h, cpu: null,
        storage: false, format: "depth24plus", depth: true, mip_levels: 1,
      });
      return id;
    },

    gpuTextureCreateRgba16Float: (width, height) => {
      const id = nextId++;
      const w = Math.max(1, width | 0);
      const h = Math.max(1, height | 0);
      textures.set(id, {
        texture: null, width: w, height: h, cpu: new Uint8Array(w * h * 8),
        storage: false, format: "rgba16float", mip_levels: 1,
      });
      return id;
    },

    gpuTextureCreateCubeRgba8: (size) => {
      const id = nextId++;
      const s = Math.max(1, size | 0);
      textures.set(id, {
        texture: null, width: s, height: s, cpu: new Uint8Array(s * s * 4 * 6),
        storage: false, format: "rgba8unorm", dimension: "cube",
        depth_or_layers: 6, mip_levels: 1,
      });
      return id;
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
        configured: false,
        lastTexture: null,
        input: makeInputState(),
      };
      surfaces.set(id, surface);
      attachSurfaceInput(surface);
      return id;
    },
    gpuSurfaceConfigure: (id, width, height) => {
      const s = surfaces.get(id);
      if (!s) throw new Error(`unknown GpuSurface ${id}`);
      s.width = Math.max(1, width | 0);
      s.height = Math.max(1, height | 0);
      s.canvas.width = s.width;
      s.canvas.height = s.height;
      s.configured = false;
    },
    gpuSurfacePresent: async (id) => {
      return surfaces.has(id) ? 0 : ERR_OTHER;
    },
    gpuSurfacePointer: (id) => {
      const s = surfaces.get(id);
      if (!s || !s.input) return new Uint8Array(32);
      return packPointer(s.input);
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
            alphaMode: "opaque",
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
        const bindEntries = [];
        const allBinds = [...(vsMeta.bindings || []), ...(fsMeta.bindings || [])];
        const seenBind = new Set();
        for (const b of allBinds) {
          if (seenBind.has(b.binding)) continue;
          seenBind.add(b.binding);
          const visibility =
            (vsMeta.bindings || []).some((x) => x.binding === b.binding)
              ? GPUShaderStage.VERTEX
              : 0;
          const fragVis =
            (fsMeta.bindings || []).some((x) => x.binding === b.binding)
              ? GPUShaderStage.FRAGMENT
              : 0;
          const vis = (visibility | fragVis) || (GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT);
          if (b.kind === "uniform") {
            bindEntries.push({ binding: b.binding, visibility: vis, buffer: { type: "uniform" } });
          } else if (b.kind === "storage") {
            bindEntries.push({
              binding: b.binding,
              visibility: vis,
              buffer: { type: b.read_write ? "storage" : "read-only-storage" },
            });
          } else if (b.kind === "sampler") {
            bindEntries.push({ binding: b.binding, visibility: vis, sampler: { type: "filtering" } });
          } else if (b.kind === "texture") {
            bindEntries.push({
              binding: b.binding,
              visibility: vis,
              texture: { sampleType: "float" },
            });
          } else if (b.kind === "depth_texture") {
            bindEntries.push({
              binding: b.binding,
              visibility: vis,
              texture: { sampleType: "depth" },
            });
          } else if (b.kind === "storage_texture") {
            bindEntries.push({
              binding: b.binding,
              visibility: vis,
              storageTexture: { access: "write-only", format: "rgba8unorm" },
            });
          }
        }
        const bgl = bindEntries.length
          ? dev.createBindGroupLayout({ entries: bindEntries })
          : null;
        const layout = bgl
          ? dev.createPipelineLayout({ bindGroupLayouts: [bgl] })
          : "auto";
        const attribs = (vsMeta.vertex_layout || []).map((a) => ({
          shaderLocation: a.location | 0,
          offset: a.offset | 0,
          format: a.format || "float32x4",
        }));
        const stride = (vsMeta.vertex_stride | 0) || 0;
        const vertexBuffers = stride > 0 && attribs.length > 0
          ? [{ arrayStride: stride, stepMode: "vertex", attributes: attribs }]
          : [];
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
          pipeline, vsMeta, fsMeta, bgl,
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

    gpuRenderDraw: async (
      surfaceId, pipelineId, vertexBufferId, vertexCount,
      uniforms, clearR, clearG, clearB, clearA,
    ) => {
      return await host.gpuRenderDrawEx(
        surfaceId, pipelineId, vertexBufferId, vertexCount, 1,
        uniforms, clearR, clearG, clearB, clearA, -1, 0,
      );
    },

    gpuRenderDrawEx: async (
      surfaceId, pipelineId, vertexBufferId, vertexCount, instanceCount,
      uniforms, clearR, clearG, clearB, clearA, depthTextureId, loadOp,
    ) => {
      try {
        const s = surfaces.get(surfaceId);
        if (!s) throw new Error(`unknown GpuSurface ${surfaceId}`);
        const rp = renderPipelines.get(pipelineId);
        if (!rp) throw new Error(`unknown GpuRenderPipeline ${pipelineId}`);
        const dev = await ensureDevice();
        if (!s.context) {
          s.context = s.canvas.getContext("webgpu");
          if (!s.context) throw new Error("canvas webgpu context unavailable");
        }
        if (!s.configured) {
          s.context.configure({
            device: dev,
            format: navigator.gpu.getPreferredCanvasFormat(),
            alphaMode: "opaque",
          });
          s.configured = true;
        }
        const view = s.context.getCurrentTexture().createView();
        const encoder = dev.createCommandEncoder();
        const passDesc = {
          colorAttachments: [{
            view,
            clearValue: {
              r: +clearR || 0,
              g: +clearG || 0,
              b: +clearB || 0,
              a: clearA === undefined || clearA === null ? 1 : +clearA,
            },
            loadOp: (loadOp | 0) === 1 ? "load" : "clear",
            storeOp: "store",
          }],
        };
        if (rp.depthEnabled && depthTextureId >= 0) {
          const dt = textures.get(depthTextureId);
          if (!dt) throw new Error(`unknown depth GpuTexture ${depthTextureId}`);
          await ensureDepthTexture(dev, dt);
          passDesc.depthStencilAttachment = {
            view: dt.texture.createView(),
            depthClearValue: 1.0,
            depthLoadOp: (loadOp | 0) === 1 ? "load" : "clear",
            depthStoreOp: "store",
          };
        }
        const pass = encoder.beginRenderPass(passDesc);
        pass.setPipeline(rp.pipeline);
        const vb = buffers.get(vertexBufferId);
        if (vb && (rp.vsMeta.vertex_stride | 0) > 0) {
          const gpuVb = await ensureGpuBuffer(dev, vb, GPUBufferUsage.VERTEX);
          pass.setVertexBuffer(0, gpuVb);
        }
        const bg = await buildRenderBindGroup(dev, rp, toU8(uniforms));
        if (bg) pass.setBindGroup(0, bg);
        pass.draw(Math.max(0, vertexCount | 0), Math.max(1, instanceCount | 0));
        pass.end();
        dev.queue.submit([encoder.finish()]);
        await dev.queue.onSubmittedWorkDone();
        return 0;
      } catch (e) {
        console.error("Dream gpuRenderDrawEx:", e);
        return classifyErr(e);
      }
    },

    gpuRenderDrawIndexed: async (
      surfaceId, pipelineId, vertexBufferId, indexBufferId, indexCount,
      uniforms, clearR, clearG, clearB, clearA,
    ) => {
      return await host.gpuRenderDrawIndexedEx(
        surfaceId, pipelineId, vertexBufferId, indexBufferId, indexCount, 1,
        uniforms, clearR, clearG, clearB, clearA, -1, 0,
      );
    },

    gpuRenderDrawIndexedEx: async (
      surfaceId, pipelineId, vertexBufferId, indexBufferId, indexCount, instanceCount,
      uniforms, clearR, clearG, clearB, clearA, depthTextureId, loadOp,
    ) => {
      try {
        const s = surfaces.get(surfaceId);
        if (!s) throw new Error(`unknown GpuSurface ${surfaceId}`);
        const rp = renderPipelines.get(pipelineId);
        if (!rp) throw new Error(`unknown GpuRenderPipeline ${pipelineId}`);
        const dev = await ensureDevice();
        if (!s.context) {
          s.context = s.canvas.getContext("webgpu");
          if (!s.context) throw new Error("canvas webgpu context unavailable");
        }
        if (!s.configured) {
          s.context.configure({
            device: dev,
            format: navigator.gpu.getPreferredCanvasFormat(),
            alphaMode: "opaque",
          });
          s.configured = true;
        }
        const view = s.context.getCurrentTexture().createView();
        const encoder = dev.createCommandEncoder();
        const passDesc = {
          colorAttachments: [{
            view,
            clearValue: {
              r: +clearR || 0,
              g: +clearG || 0,
              b: +clearB || 0,
              a: clearA === undefined || clearA === null ? 1 : +clearA,
            },
            loadOp: (loadOp | 0) === 1 ? "load" : "clear",
            storeOp: "store",
          }],
        };
        if (rp.depthEnabled && depthTextureId >= 0) {
          const dt = textures.get(depthTextureId);
          if (!dt) throw new Error(`unknown depth GpuTexture ${depthTextureId}`);
          await ensureDepthTexture(dev, dt);
          passDesc.depthStencilAttachment = {
            view: dt.texture.createView(),
            depthClearValue: 1.0,
            depthLoadOp: (loadOp | 0) === 1 ? "load" : "clear",
            depthStoreOp: "store",
          };
        }
        const pass = encoder.beginRenderPass(passDesc);
        pass.setPipeline(rp.pipeline);
        const vb = buffers.get(vertexBufferId);
        if (vb && (rp.vsMeta.vertex_stride | 0) > 0) {
          const gpuVb = await ensureGpuBuffer(dev, vb, GPUBufferUsage.VERTEX);
          pass.setVertexBuffer(0, gpuVb);
        }
        const ib = buffers.get(indexBufferId);
        if (!ib) throw new Error(`unknown index GpuBuffer ${indexBufferId}`);
        const gpuIb = await ensureGpuBuffer(dev, ib, GPUBufferUsage.INDEX);
        pass.setIndexBuffer(gpuIb, "uint32");
        const bg = await buildRenderBindGroup(dev, rp, toU8(uniforms));
        if (bg) pass.setBindGroup(0, bg);
        pass.drawIndexed(Math.max(0, indexCount | 0), Math.max(1, instanceCount | 0));
        pass.end();
        dev.queue.submit([encoder.finish()]);
        await dev.queue.onSubmittedWorkDone();
        return 0;
      } catch (e) {
        console.error("Dream gpuRenderDrawIndexedEx:", e);
        return classifyErr(e);
      }
    },

    gpuPassBegin: () => {
      const id = nextId++;
      passes.set(id, { ops: [] });
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
        passes.delete(passId);
        if (ops.length === 0) return 0;
        const dev = await ensureDevice();
        const encoder = dev.createCommandEncoder();
        for (const op of ops) {
          const pipe = await getPipeline(dev, op.kernel);
          if (op.kind === "dispatch") {
            const bg = await buildBindGroup(
              dev, pipe, op.bufferIds, op.textureIds, op.samplerIds,
              op.ex, op.ey, op.ez, op.uniforms,
            );
            encodeDispatch(encoder, pipe, bg, op.ex, op.ey, op.ez);
          } else {
            const bg = await buildBindGroup(
              dev, pipe, op.bufferIds, op.textureIds, op.samplerIds, 1, 1, 1, [],
            );
            await encodeDispatchIndirect(
              dev, encoder, pipe, bg, op.indirectId, op.indirectOffset,
            );
          }
        }
        dev.queue.submit([encoder.finish()]);
        await dev.queue.onSubmittedWorkDone();
        return 0;
      } catch (e) {
        console.error("Dream gpuPassSubmit:", e);
        return classifyErr(e);
      }
    },
  };

  return host;
}

export { makeGpuHost };
