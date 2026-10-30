// The same Vello implementation runs natively and in the separate drawing WASM host.
import { drawingGpuBridge } from "./gpu_drawing.js";
import { isNode } from "../platform.js";

let drawingEngine;
async function loadDrawingEngine() {
  if (!drawingEngine) drawingEngine = (async () => {
    const module = await import(new URL("./dream_drawing.js", import.meta.url).href);
    if (isNode) {
      const { readFile } = await import("node:fs/promises");
      const bytes = await readFile(new URL("./dream_drawing_bg.wasm", import.meta.url));
      await module.default({ module_or_path: bytes });
    } else {
      await module.default();
    }
    return module;
  })();
  return drawingEngine;
}

export function makeDrawingHost(getInstance) {
  const bridge = drawingGpuBridge(getInstance);
  if (bridge && !isNode) bridge.createDevice = async () => {
    const module = await loadDrawingEngine();
    const [context, device] = await module.gpu_context();
    return { device, context, release: () => module.gpu_context_release(context) };
  };
  let engine;
  let error = "";
  const guard = (fallback, body) => (...args) => {
    try { return body(...args); }
    catch (failure) { error = failure.message || String(failure); return fallback; }
  };
  const asyncGuard = (fallback, body) => async (...args) => {
    try { engine = await loadDrawingEngine(); return await body(...args); }
    catch (failure) { error = failure.message || String(failure); return fallback; }
  };
  return {
    drawingCreate: async (width, height, backend) => {
      try {
        engine = await loadDrawingEngine();
        return await engine.create(width, height, backend);
      } catch (failure) { error = failure.message || String(failure); return 0; }
    },
    drawingLastError: () => error,
    drawingRender: guard(0, (handle, values, assets) => { engine.render(handle, new Float32Array(values), new Int32Array(assets)); return 1; }),
    drawingPixels: async handle => {
      try { return await engine.pixels(handle); }
      catch (failure) { error = failure.message || String(failure); return []; }
    },
    drawingCopyGpu: guard(0, (handle, texture, width, height) => {
      const backend = engine.backend(handle);
      if (backend === 0) throw new Error("EINVAL\ndrawing surface is closed");
      if (backend === 1) return -1;
      if (!bridge?.copy) throw new Error("EUNAVAILABLE\nGPU host is unavailable");
      try {
        bridge.copy(texture, width, height, context => engine.gpu_source(handle, context));
      } catch (failure) {
        if (failure.code) throw new Error(failure.code + "\n" + failure.detail);
        throw failure;
      }
      return 1;
    }),
    drawingBackend: guard(0, handle => engine.backend(handle)),
    drawingRelease: guard(undefined, handle => engine.release(handle)),
    drawingImageDecode: asyncGuard(0, bytes => engine.image_decode(new Uint8Array(bytes))),
    drawingImageRgba: asyncGuard(0, (width, height, bytes) => engine.image_rgba(width, height, new Uint8Array(bytes))),
    drawingTextCreate: asyncGuard(0, (text, family, size, weight, width, alignment) => engine.text_create(text, family, size, weight, width, alignment)),
    drawingFontRegister: asyncGuard("", bytes => engine.font_register(new Uint8Array(bytes))),
    drawingAssetMetrics: guard([], handle => engine.asset_metrics(handle)),
    drawingAssetRelease: guard(undefined, handle => engine.asset_release(handle)),
  };
}
