// WebGPU host for `system.gpu` (imports declared in crates/dream-stdlib/src/system/gpu/gpu_host.dream).
// Shaders come only from the `wgsl` target of generated bundles; this host never compiles
// anything but that WGSL. Canvas surfaces attach to `system.window` canvases.
import { gpuParseBundle, gpuScanWgslBindings, gpuVerifyTarget } from "./gpu_bundle.js";
import {
  GPU_ARG, GPU_BUFFER, GPU_FEATURE, GPU_MAP_READ, GPU_STAGE, GPU_TEXTURE, GPU_TEXTURE_FORMATS,
  GPU_VISIBILITY, GpuFailure, gpuPipelineLayout, gpuRecordCommands,
} from "./gpu_commands.js";
import { windowSurfaceCanvas } from "./window.js";

const GPU_BACKEND_AUTO = 0;
const GPU_BACKEND_WEBGPU = 6;
const GPU_BACKEND_NAMES = ["Auto", "Dx12", "Dx11", "Vulkan", "Metal", "Cuda", "WebGpu"];
const GPU_TOPOLOGIES = ["triangle-list", "triangle-strip", "line-list", "line-strip", "point-list"];
const GPU_CULL_MODES = ["none", "front", "back"];
const GPU_COMPARES = ["never", "less", "less-equal", "equal", "greater-equal", "greater", "not-equal", "always"];
const GPU_VERTEX_FORMATS = ["float32", "float32x2", "float32x3", "float32x4", "sint32", "uint32", "unorm8x4"];
const GPU_FILTERS = ["nearest", "linear"];
const GPU_ADDRESS_MODES = ["clamp-to-edge", "repeat", "mirror-repeat"];
const GPU_BLENDS = [
  null,
  { color: { srcFactor: "src-alpha", dstFactor: "one-minus-src-alpha" }, alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha" } },
  { color: { srcFactor: "one", dstFactor: "one-minus-src-alpha" }, alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha" } },
  { color: { srcFactor: "one", dstFactor: "one" }, alpha: { srcFactor: "one", dstFactor: "one" } },
];
// Adapter features requested when present, with the GpuFeature bit each one grants.
const GPU_OPTIONAL_FEATURES = [["subgroups", GPU_FEATURE.SUBGROUPS], ["shader-f16", GPU_FEATURE.FLOAT16]];
// The bundle's capability names (docs/internals/18-gpu-shader-bundle.md), by GpuFeature.
const GPU_CAPABILITY_FEATURES = new Map([
  ["raytracing", GPU_FEATURE.RAY_TRACING],
  ["rayquery", GPU_FEATURE.RAY_QUERY],
  ["descriptor_handle", GPU_FEATURE.BINDLESS],
  ["subgroup_basic", GPU_FEATURE.SUBGROUPS],
  ["cooperative_vector", GPU_FEATURE.COOPERATIVE_VECTORS],
  ["cooperative_matrix", GPU_FEATURE.COOPERATIVE_MATRICES],
  ["half", GPU_FEATURE.FLOAT16],
  ["atomic64", GPU_FEATURE.ATOMICS64],
]);

function gpuNextFrame() {
  return new Promise(resolve => {
    if (typeof globalThis.requestAnimationFrame === "function") globalThis.requestAnimationFrame(() => resolve());
    else setTimeout(resolve, 0);
  });
}

function gpuBytes(data) {
  return data instanceof Uint8Array ? data : Uint8Array.from(data || []);
}


export function makeGpuHost() {
  // Every live handle: devices, buffers, textures, samplers, libraries, pipelines and surfaces.
  const table = new Map();
  let nextHandle = 1;
  let lastError = "";

  const fail = (code, detail, result = 0) => {
    lastError = code + "\n" + detail;
    return result;
  };
  const failWith = (error, result = 0) => {
    if (error instanceof GpuFailure) return fail(error.code, error.detail, result);
    return fail("EVALIDATION", error?.message || String(error), result);
  };
  const register = record => {
    const handle = nextHandle++;
    table.set(handle, record);
    return handle;
  };
  const live = (dev) => {
    if (dev.lost !== null) throw new GpuFailure("EDEVICELOST", dev.lost);
  };
  const deviceOf = handle => {
    const record = table.get(handle);
    if (record?.type !== "device") throw new GpuFailure("EINVALID", "GPU device " + handle + " was released");
    live(record);
    return record;
  };
  const recordOf = (handle, type, what) => {
    const record = table.get(handle);
    if (!record || (type && record.type !== type)) throw new GpuFailure("EINVALID", "the " + what + " was released");
    live(record.dev);
    return record;
  };
  const resourceOn = dev => handle => {
    const record = table.get(handle);
    if (!record || record.type === "device") throw new GpuFailure("EINVALID", "GPU handle " + handle + " was released");
    if (record.dev !== dev) throw new GpuFailure("EWRONGDEVICE", "GPU handle " + handle);
    return record;
  };
  // Runs a synchronous import body; host failures become `fallback` plus `gpuLastError`.
  const guard = (fallback, body) => {
    try {
      return body();
    } catch (error) {
      return failWith(error, fallback);
    }
  };

  const createDevice = async (backend, compiled) => {
    if (backend !== GPU_BACKEND_AUTO && backend !== GPU_BACKEND_WEBGPU) {
      if (compiled !== 0 && (compiled & (1 << backend)) === 0) return fail("ENOTCOMPILED", backend);
      const name = GPU_BACKEND_NAMES[backend] || String(backend);
      return fail("EUNAVAILABLE", "GpuBackend." + name + " is not available in a JavaScript host; browsers run GpuBackend.WebGpu");
    }
    if (compiled !== 0 && (compiled & (1 << GPU_BACKEND_WEBGPU)) === 0) return fail("ENOTCOMPILED", GPU_BACKEND_WEBGPU);
    const gpu = globalThis.navigator?.gpu;
    if (!gpu?.requestAdapter) return fail("ENODEVICE", "WebGPU is not available: navigator.gpu is missing");
    let adapter;
    try {
      adapter = await gpu.requestAdapter();
    } catch (error) {
      return fail("ENODEVICE", "WebGPU adapter request failed: " + (error?.message || error));
    }
    if (!adapter) return fail("ENODEVICE", "WebGPU found no adapter");
    const granted = GPU_OPTIONAL_FEATURES.filter(([name]) => adapter.features?.has?.(name));
    let device;
    try {
      device = await adapter.requestDevice({ requiredFeatures: granted.map(([name]) => name) });
    } catch (error) {
      return fail("ENODEVICE", "WebGPU device request failed: " + (error?.message || error));
    }
    const adapterName = adapter.info?.description || [adapter.info?.vendor, adapter.info?.device].filter(Boolean).join(" ");
    const dev = { type: "device", device, adapterName, lost: null, uncaptured: null, libraries: new Set(), features: 0 };
    dev.dev = dev;
    for (const [name, bit] of granted) if (device.features?.has?.(name) !== false) dev.features |= 1 << bit;
    device.lost?.then?.(info => {
      if (dev.lost === null) dev.lost = info?.message || "the WebGPU device was lost";
    });
    device.onuncapturederror = event => {
      dev.uncaptured ??= event?.error?.message || "WebGPU reported an error";
    };
    return register(dev);
  };

  const loadLibrary = (deviceHandle, bundle) => {
    const dev = deviceOf(deviceHandle);
    let parsed;
    try {
      parsed = gpuParseBundle(gpuBytes(bundle));
    } catch (error) {
      throw new GpuFailure("EINVALID", "malformed shader bundle: " + error.message);
    }
    const target = parsed.targets.find(t => t.name === "wgsl");
    if (!target) throw new GpuFailure("ENOTCOMPILED", GPU_BACKEND_WEBGPU);
    if (target.codeFormat !== "wgsl") throw new GpuFailure("EINVALID", "the wgsl target holds " + target.codeFormat + " code");
    const library = { type: "library", dev, target, modules: new Map(), bad: null, verified: null };
    library.verified = gpuVerifyTarget(target).then(reason => {
      library.bad = reason;
    }, error => {
      library.bad = "SHA-256 verification failed: " + (error?.message || error);
    });
    dev.libraries.add(library);
    return register(library);
  };

  // The GPUShaderModule and binding declarations for one entry; blobs shared by entries share one module.
  const shaderOf = (library, name, stage) => {
    if (library.bad) throw new GpuFailure("EINVALID", library.bad);
    const entry = library.target.entries.find(e => e.name === name);
    // A shader whose @gpu_targets leave out WebGPU (GpuBackend 6) has no WGSL entry.
    if (!entry) throw new GpuFailure("ENOTCOMPILED", "6");
    if (entry.stage !== stage) throw new GpuFailure("EVALIDATION", name + " is a " + entry.stage + " shader, not " + stage);
    for (const capability of entry.capabilities) {
      const feature = GPU_CAPABILITY_FEATURES.get(capability);
      if (feature === undefined) throw new GpuFailure("EINVALID", name + " needs the unknown capability " + capability);
      if ((library.dev.features & (1 << feature)) === 0) throw new GpuFailure("EUNSUPPORTED", feature);
    }
    let shader = library.modules.get(entry.code.offset);
    if (!shader) {
      let code;
      try {
        code = new TextDecoder("utf-8", { fatal: true }).decode(entry.code.bytes);
      } catch (_) {
        throw new GpuFailure("EINVALID", "the WGSL of " + name + " is not UTF-8");
      }
      shader = { module: library.dev.device.createShaderModule({ code }), decls: gpuScanWgslBindings(code) };
      library.modules.set(entry.code.offset, shader);
    }
    return { entry, ...shader };
  };

  const renderPipelineDesc = (desc, vertex, fragment, layout) => {
    const word = i => {
      if (i >= desc.length) throw new GpuFailure("EVALIDATION", "the render pipeline description is truncated");
      return desc[i];
    };
    const [colorCode, topology, cull, blend, depthCode, depthCompare, depthWrite, samples, layoutCount] =
      Array.from({ length: 9 }, (_, i) => word(i));
    const color = GPU_TEXTURE_FORMATS[colorCode];
    if (!color || colorCode < 0 || colorCode > 8) throw new GpuFailure("EINVALID", "unsupported floating-point shader color target " + colorCode);
    const buffers = [];
    let at = 9;
    for (let i = 0; i < layoutCount; i++) {
      const arrayStride = word(at);
      const stepMode = word(at + 1) ? "instance" : "vertex";
      const count = word(at + 2);
      at += 3;
      const attributes = [];
      for (let j = 0; j < count; j++, at += 3) {
        const format = GPU_VERTEX_FORMATS[word(at + 1)];
        if (!format) throw new GpuFailure("EINVALID", "unknown vertex format " + word(at + 1));
        attributes.push({ shaderLocation: word(at), format, offset: word(at + 2) });
      }
      buffers.push({ arrayStride, stepMode, attributes });
    }
    const target = { format: color[0] };
    if (GPU_BLENDS[blend]) target.blend = GPU_BLENDS[blend];
    const out = {
      layout,
      vertex: { module: vertex.module, entryPoint: vertex.entry.name, buffers },
      fragment: { module: fragment.module, entryPoint: fragment.entry.name, targets: [target] },
      primitive: { topology: GPU_TOPOLOGIES[topology] || "triangle-list", cullMode: GPU_CULL_MODES[cull] || "none" },
      multisample: { count: samples > 0 ? samples : 1 },
    };
    if (depthCode > 0) {
      const depth = GPU_TEXTURE_FORMATS[depthCode - 1];
      if (!depth) throw new GpuFailure("EINVALID", "unknown depth format " + (depthCode - 1));
      out.depthStencil = { format: depth[0], depthCompare: GPU_COMPARES[depthCompare] || "less", depthWriteEnabled: depthWrite !== 0 };
    }
    return out;
  };

  // Copies `size` bytes at `offset` out of `buffer` through a mappable staging buffer.
  const readBack = async (dev, record, offset, size) => {
    const start = offset & ~3;
    const span = Math.ceil((offset + size - start) / 4) * 4;
    if (start + span > record.allocated) throw new GpuFailure("EINVALID", "the read range lies outside the buffer");
    const staging = dev.device.createBuffer({ size: span, usage: GPU_BUFFER.MAP_READ | GPU_BUFFER.COPY_DST });
    try {
      const encoder = dev.device.createCommandEncoder();
      encoder.copyBufferToBuffer(record.obj, start, staging, 0, span);
      dev.device.queue.submit([encoder.finish()]);
      await staging.mapAsync(GPU_MAP_READ);
      live(dev);
      const bytes = new Uint8Array(staging.getMappedRange()).slice(offset - start, offset - start + size);
      staging.unmap();
      return bytes;
    } finally {
      staging.destroy();
    }
  };

  const textureExtent = (record, mip) => [Math.max(1, record.width >> mip), Math.max(1, record.height >> mip)];

  const submit = async (deviceHandle, commands, data) => {
    const dev = deviceOf(deviceHandle);
    await Promise.all([...dev.libraries].map(l => l.verified));
    live(dev);
    const pipelineOf = handle => {
      const record = resourceOn(dev)(handle);
      if (record.type !== "pipeline") throw new GpuFailure("EINVALID", "GPU handle " + handle + " is not a pipeline");
      const bad = record.libraries.find(l => l.bad);
      if (bad) throw new GpuFailure("EINVALID", bad.bad);
      return record;
    };
    const device = dev.device;
    const transient = [];
    let recordError = null;
    device.pushErrorScope?.("out-of-memory");
    device.pushErrorScope?.("validation");
    try {
      device.queue.submit([gpuRecordCommands(dev, commands, gpuBytes(data), resourceOn(dev), pipelineOf, transient)]);
    } catch (error) {
      recordError = error;
    }
    const validation = await device.popErrorScope?.();
    const memory = await device.popErrorScope?.();
    if (!recordError) await device.queue.onSubmittedWorkDone?.();
    for (const buffer of transient) buffer.destroy();
    if (recordError) throw recordError;
    live(dev);
    if (memory) throw new GpuFailure("ENOMEM", memory.message);
    if (validation) throw new GpuFailure("EVALIDATION", validation.message);
    if (dev.uncaptured !== null) {
      const message = dev.uncaptured;
      dev.uncaptured = null;
      throw new GpuFailure("EVALIDATION", message);
    }
    return 0;
  };

  const release = handle => {
    const record = table.get(handle);
    if (!record) return;
    table.delete(handle);
    switch (record.type) {
      case "device":
        for (const [other, owned] of table) if (owned.dev === record) table.delete(other);
        record.lost ??= "the device was released";
        record.device.destroy?.();
        break;
      case "buffer":
        record.obj.destroy?.();
        break;
      case "texture":
        if (!record.surface) record.obj.destroy?.();
        break;
      case "library":
        record.dev.libraries.delete(record);
        break;
      case "surface":
        if (record.frame) table.delete(record.frame);
        record.context.unconfigure?.();
        break;
    }
  };

  const dropFrame = surface => {
    if (surface.frame) table.delete(surface.frame);
    surface.frame = 0;
  };

  const asyncGuard = /* @__NO_SIDE_EFFECTS__ */ (fallback, body) => async (...args) => {
    try {
      return await body(...args);
    } catch (error) {
      return failWith(error, fallback);
    }
  };

  return {
    gpuDeviceCreate: asyncGuard(0, createDevice),
    gpuLastError: () => lastError || "EVALIDATION\nthe GPU host reported no error",
    gpuDeviceBackend: handle => (table.get(handle)?.type === "device" ? GPU_BACKEND_WEBGPU : 0),
    gpuDeviceAdapterName: handle => guard("", () => deviceOf(handle).adapterName || ""),
    gpuDeviceFeatures: handle => {
      const record = table.get(handle);
      return record?.type === "device" ? record.features : 0;
    },
    gpuDeviceCheck: handle => guard(1, () => {
      deviceOf(handle);
      return 0;
    }),
    gpuLibraryLoad: (device, bundle) => guard(0, () => loadLibrary(device, bundle)),
    gpuBufferCreate: (device, byteSize, usage) => guard(0, () => {
      const dev = deviceOf(device);
      if (!(byteSize >= 0)) throw new GpuFailure("EINVALID", "buffer size must not be negative");
      let flags = GPU_BUFFER.COPY_SRC | GPU_BUFFER.COPY_DST;
      if (usage & 1) flags |= GPU_BUFFER.STORAGE;
      if (usage & 2) flags |= GPU_BUFFER.VERTEX;
      if (usage & 4) flags |= GPU_BUFFER.INDEX;
      if (usage & 8) flags |= GPU_BUFFER.INDIRECT;
      // WebGPU sizes and copies buffers in whole 4-byte words.
      const allocated = Math.max(4, Math.ceil(byteSize / 4) * 4);
      const obj = dev.device.createBuffer({ size: allocated, usage: flags });
      return register({ type: "buffer", dev, obj, size: byteSize, allocated });
    }),
    gpuBufferWrite: (buffer, byteOffset, data) => guard(1, () => {
      const record = recordOf(buffer, "buffer", "buffer");
      const bytes = gpuBytes(data);
      if (byteOffset < 0 || byteOffset + bytes.length > record.size) throw new GpuFailure("EINVALID", "the write range lies outside the buffer");
      if (byteOffset & 3) throw new GpuFailure("EINVALID", "WebGPU writes buffers at 4-byte aligned offsets");
      let words = bytes;
      if (bytes.length & 3) {
        // Only a write that ends the buffer may be padded: the padding then lands in the
        // allocation's slack rather than over live data.
        if (byteOffset + bytes.length !== record.size) {
          throw new GpuFailure("EINVALID", "WebGPU writes whole 4-byte words; the write length must be a multiple of 4");
        }
        words = new Uint8Array(Math.ceil(bytes.length / 4) * 4);
        words.set(bytes);
      }
      record.dev.device.queue.writeBuffer(record.obj, byteOffset, words);
      return 0;
    }),
    gpuBufferRead: asyncGuard([], async (buffer, byteOffset, byteSize) => {
      const record = recordOf(buffer, "buffer", "buffer");
      if (byteOffset < 0 || byteSize < 0 || byteOffset + byteSize > record.size) throw new GpuFailure("EINVALID", "the read range lies outside the buffer");
      return readBack(record.dev, record, byteOffset, byteSize);
    }),
    gpuTextureCreate: (device, desc) => guard(0, () => {
      const dev = deviceOf(device);
      const [width, height, layers, mips, formatCode, usage, samples] = desc;
      const format = GPU_TEXTURE_FORMATS[formatCode];
      if (!format || desc.length < 7) throw new GpuFailure("EINVALID", "unknown texture format " + formatCode);
      if (!(width > 0 && height > 0)) throw new GpuFailure("EINVALID", "texture width and height must be positive");
      let flags = GPU_TEXTURE.TEXTURE_BINDING;
      if (format[1] > 0) flags |= GPU_TEXTURE.COPY_SRC | GPU_TEXTURE.COPY_DST;
      if ((usage & 1) || format[1] === 0) flags |= GPU_TEXTURE.RENDER_ATTACHMENT;
      if (usage & 2) flags |= GPU_TEXTURE.STORAGE_BINDING;
      const obj = dev.device.createTexture({
        size: [width, height, Math.max(1, layers)],
        mipLevelCount: Math.max(1, mips),
        sampleCount: Math.max(1, samples),
        format: format[0],
        usage: flags,
      });
      return register({ type: "texture", dev, obj, width, height, format: format[0], texel: format[1] });
    }),
    gpuTextureWrite: (texture, mip, layer, data) => guard(1, () => {
      const record = recordOf(texture, "texture", "texture");
      if (record.surface || record.texel === 0) throw new GpuFailure("EINVALID", "this texture cannot be written from the CPU");
      const [width, height] = textureExtent(record, mip);
      const bytes = gpuBytes(data);
      if (bytes.length !== width * height * record.texel) {
        throw new GpuFailure("EINVALID", "mip " + mip + " needs " + width * height * record.texel + " bytes, got " + bytes.length);
      }
      record.dev.device.queue.writeTexture(
        { texture: record.obj, mipLevel: mip, origin: { x: 0, y: 0, z: layer } },
        bytes,
        { bytesPerRow: width * record.texel, rowsPerImage: height },
        { width, height, depthOrArrayLayers: 1 },
      );
      return 0;
    }),
    gpuTextureRead: asyncGuard([], async (texture, mip, layer) => {
      const record = recordOf(texture, "texture", "texture");
      if (record.texel === 0) throw new GpuFailure("EINVALID", "depth textures cannot be read back in WebGPU");
      const dev = record.dev;
      const [width, height] = textureExtent(record, mip);
      const row = width * record.texel;
      // Texture-to-buffer copies need 256-byte aligned rows.
      const stride = Math.ceil(row / 256) * 256;
      const staging = dev.device.createBuffer({ size: stride * height, usage: GPU_BUFFER.MAP_READ | GPU_BUFFER.COPY_DST });
      try {
        const encoder = dev.device.createCommandEncoder();
        encoder.copyTextureToBuffer(
          { texture: record.obj, mipLevel: mip, origin: { x: 0, y: 0, z: layer } },
          { buffer: staging, bytesPerRow: stride, rowsPerImage: height },
          { width, height, depthOrArrayLayers: 1 },
        );
        dev.device.queue.submit([encoder.finish()]);
        await staging.mapAsync(GPU_MAP_READ);
        live(dev);
        const mapped = new Uint8Array(staging.getMappedRange());
        const out = new Uint8Array(row * height);
        for (let y = 0; y < height; y++) out.set(mapped.subarray(y * stride, y * stride + row), y * row);
        staging.unmap();
        return out;
      } finally {
        staging.destroy();
      }
    }),
    gpuSamplerCreate: (device, desc) => guard(0, () => {
      const dev = deviceOf(device);
      if (desc.length < 8) throw new GpuFailure("EINVALID", "the sampler description is truncated");
      const [minFilter, magFilter, mipFilter, u, v, w, compare, anisotropy] = desc;
      const options = {
        minFilter: GPU_FILTERS[minFilter] || "nearest",
        magFilter: GPU_FILTERS[magFilter] || "nearest",
        mipmapFilter: GPU_FILTERS[mipFilter] || "nearest",
        addressModeU: GPU_ADDRESS_MODES[u] || "clamp-to-edge",
        addressModeV: GPU_ADDRESS_MODES[v] || "clamp-to-edge",
        addressModeW: GPU_ADDRESS_MODES[w] || "clamp-to-edge",
      };
      // WebGPU allows anisotropy only when every filter is linear.
      if (anisotropy > 1 && minFilter === 1 && magFilter === 1 && mipFilter === 1) options.maxAnisotropy = anisotropy;
      if (compare > 0) options.compare = GPU_COMPARES[compare - 1];
      return register({ type: "sampler", dev, obj: dev.device.createSampler(options) });
    }),
    gpuDescriptorHandle: () => fail("EUNSUPPORTED", GPU_FEATURE.BINDLESS, 0xffffffffffffffffn),
    gpuComputePipelineCreate: (library, entry) => guard(0, () => {
      const lib = recordOf(library, "library", "shader library");
      const shader = shaderOf(lib, entry, "compute");
      const { groups, layout } = gpuPipelineLayout(lib.dev.device, [
        { stage: GPU_STAGE.COMPUTE, visibility: GPU_VISIBILITY.COMPUTE, ...shader },
      ]);
      const obj = lib.dev.device.createComputePipeline({ layout, compute: { module: shader.module, entryPoint: entry } });
      return register({ type: "pipeline", kind: "compute", dev: lib.dev, obj, groups, libraries: [lib] });
    }),
    gpuRenderPipelineCreate: (vertexLibrary, vertexEntry, fragmentLibrary, fragmentEntry, desc) => guard(0, () => {
      const vlib = recordOf(vertexLibrary, "library", "vertex shader library");
      const flib = recordOf(fragmentLibrary, "library", "fragment shader library");
      if (vlib.dev !== flib.dev) throw new GpuFailure("EWRONGDEVICE", "fragment shader library");
      const vertex = shaderOf(vlib, vertexEntry, "vertex");
      const fragment = shaderOf(flib, fragmentEntry, "fragment");
      const { groups, layout } = gpuPipelineLayout(vlib.dev.device, [
        { stage: GPU_STAGE.VERTEX, visibility: GPU_VISIBILITY.VERTEX, ...vertex },
        { stage: GPU_STAGE.FRAGMENT, visibility: GPU_VISIBILITY.FRAGMENT, ...fragment },
      ]);
      const obj = vlib.dev.device.createRenderPipeline(renderPipelineDesc(desc, vertex, fragment, layout));
      return register({ type: "pipeline", kind: "render", dev: vlib.dev, obj, groups, libraries: [vlib, flib] });
    }),
    gpuRayTracingPipelineCreate: () => fail("EUNSUPPORTED", GPU_FEATURE.RAY_TRACING),
    gpuBottomLevelBuild: async () => fail("EUNSUPPORTED", GPU_FEATURE.RAY_TRACING),
    gpuTopLevelBuild: async () => fail("EUNSUPPORTED", GPU_FEATURE.RAY_TRACING),
    gpuMlpUpload: () => fail("EUNSUPPORTED", GPU_FEATURE.COOPERATIVE_VECTORS),
    gpuSurfaceCreate: (device, window, _vsync) => guard(0, () => {
      const dev = deviceOf(device);
      // The window chunk is absent from selective runtimes of programs that never open a window.
      const canvas = typeof windowSurfaceCanvas === "function" ? windowSurfaceCanvas(window) : null;
      if (!canvas) throw new GpuFailure("ESURFACE", "window " + window + " has no canvas");
      const context = canvas.getContext?.("webgpu");
      if (!context) throw new GpuFailure("ESURFACE", "the canvas has no WebGPU context");
      const preferred = globalThis.navigator?.gpu?.getPreferredCanvasFormat?.() || "bgra8unorm";
      const formatCode = GPU_TEXTURE_FORMATS.findIndex(([name]) => name === preferred);
      // Browsers present at the display's refresh rate, so the vsync hint has no effect here.
      context.configure({ device: dev.device, format: preferred, alphaMode: "opaque", usage: GPU_TEXTURE.RENDER_ATTACHMENT });
      return register({ type: "surface", dev, canvas, context, formatCode: formatCode < 0 ? 2 : formatCode, frame: 0 });
    }),
    gpuSurfaceResize: (surface, width, height) => guard(1, () => {
      const record = recordOf(surface, "surface", "surface");
      if (!(width > 0 && height > 0)) throw new GpuFailure("EINVALID", "surface width and height must be positive");
      if (record.canvas.width !== width) record.canvas.width = width;
      if (record.canvas.height !== height) record.canvas.height = height;
      return 0;
    }),
    gpuSurfaceInfo: surface => {
      const record = table.get(surface);
      if (record?.type !== "surface") return [];
      return [record.formatCode, record.canvas.width, record.canvas.height];
    },
    gpuSurfaceAcquire: asyncGuard(0, async surface => {
      const record = recordOf(surface, "surface", "surface");
      dropFrame(record);
      let texture;
      try {
        texture = record.context.getCurrentTexture();
      } catch (error) {
        throw new GpuFailure("ESURFACE", error?.message || "the canvas cannot present");
      }
      const format = GPU_TEXTURE_FORMATS[record.formatCode];
      record.frame = register({
        type: "texture", dev: record.dev, obj: texture, surface: true,
        width: record.canvas.width, height: record.canvas.height, format: format[0], texel: 0,
      });
      return record.frame;
    }),
    gpuSurfacePresent: asyncGuard(1, async surface => {
      const record = recordOf(surface, "surface", "surface");
      dropFrame(record);
      // The browser shows the canvas once the task yields; waiting for the next frame also keeps
      // a render loop from starving the page.
      await gpuNextFrame();
      live(record.dev);
      return 0;
    }),
    gpuSubmit: asyncGuard(1, submit),
    gpuRelease: handle => {
      try {
        release(handle);
      } catch (_) {
        // Releasing never fails from the program's point of view.
      }
    },
  };
}
