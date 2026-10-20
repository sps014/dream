// Translation of the `system.gpu` command stream (crates/dream-stdlib/src/system/gpu/gpu_op.dream)
// and shader parameter tables into WebGPU calls.

// GPUBufferUsage, GPUTextureUsage, GPUShaderStage and GPUMapMode values from the WebGPU spec;
// hosts without the globals (Node with a Dawn binding) still use the same numbers.
export const GPU_BUFFER = { MAP_READ: 1, COPY_SRC: 4, COPY_DST: 8, INDEX: 16, VERTEX: 32, UNIFORM: 64, STORAGE: 128, INDIRECT: 256 };
export const GPU_TEXTURE = { COPY_SRC: 1, COPY_DST: 2, TEXTURE_BINDING: 4, STORAGE_BINDING: 8, RENDER_ATTACHMENT: 16 };
export const GPU_VISIBILITY = { VERTEX: 1, FRAGMENT: 2, COMPUTE: 4 };
export const GPU_MAP_READ = 1;

// GpuArgKind.
export const GPU_ARG = { BUFFER: 1, TEXTURE: 2, SAMPLER: 3, STORAGE_TEXTURE: 4, ACCELERATION_STRUCTURE: 5, VALUE: 6, MLP_WEIGHTS: 7 };
// GpuFeature.
export const GPU_FEATURE = { RAY_TRACING: 0, RAY_QUERY: 1, BINDLESS: 2, SUBGROUPS: 3, COOPERATIVE_VECTORS: 4, COOPERATIVE_MATRICES: 5, FLOAT16: 6, ATOMICS64: 7 };
// GpuStage values that WebGPU can run.
export const GPU_STAGE = { COMPUTE: 0, VERTEX: 1, FRAGMENT: 2 };

// GpuTextureFormat order, with bytes per texel (0 for depth formats, which cannot be copied).
export const GPU_TEXTURE_FORMATS = [
  ["rgba8unorm", 4], ["rgba8unorm-srgb", 4], ["bgra8unorm", 4], ["bgra8unorm-srgb", 4],
  ["r8unorm", 1], ["r32float", 4], ["rg32float", 8], ["rgba16float", 8], ["rgba32float", 16],
  ["r32uint", 4], ["depth32float", 0], ["depth24plus-stencil8", 0],
];

/** A host failure reported to the guest as `CODE\ndetail`. */
export class GpuFailure extends Error {
  constructor(code, detail) {
    super(detail);
    this.code = code;
    this.detail = String(detail);
  }
}

/**
 * Bind group layouts for the parameter tables of every stage in one pipeline. `stages` holds
 * `{ stage, visibility, entry }`; parameters of different stages that share a
 * @group/@binding become one layout entry visible to both.
 */
export function gpuPipelineLayout(device, stages) {
  const groups = new Map();
  for (const { stage, visibility, entry } of stages) {
    entry.params.forEach((param, index) => {
      if (param.kind === GPU_ARG.ACCELERATION_STRUCTURE) throw new GpuFailure("EUNSUPPORTED", GPU_FEATURE.RAY_QUERY);
      if (param.kind === GPU_ARG.MLP_WEIGHTS) throw new GpuFailure("EUNSUPPORTED", GPU_FEATURE.COOPERATIVE_VECTORS);
      if (!groups.has(param.group)) groups.set(param.group, new Map());
      const group = groups.get(param.group);
      let slot = group.get(param.binding);
      if (!slot) {
        slot = { kind: param.kind, visibility: 0, slots: [], resource: param.webgpuLayout };
        group.set(param.binding, slot);
      } else if (slot.kind !== param.kind || slot.resource !== param.webgpuLayout) {
        throw new GpuFailure("EVALIDATION", "parameters " + slot.slots[0].param.name + " and " + param.name
          + " share @group(" + param.group + ") @binding(" + param.binding + ") with different layouts");
      }
      slot.visibility |= visibility;
      slot.slots.push({ stage, index, param });
    });
  }
  const highest = groups.size ? Math.max(...groups.keys()) : -1;
  const layouts = [];
  const bound = new Map();
  for (let g = 0; g <= highest; g++) {
    const bindings = [...(groups.get(g) || new Map())].sort((a, b) => a[0] - b[0]);
    const entries = bindings.map(([binding, slot]) => {
      if (slot.kind === GPU_ARG.VALUE) {
        const end = Math.max(...slot.slots.map(s => s.param.offset + s.param.size));
        slot.blockSize = Math.max(16, Math.ceil(end / 16) * 16);
      }
      return { binding, visibility: slot.visibility, ...slot.resource };
    });
    const object = device.createBindGroupLayout({ entries });
    layouts.push(object);
    if (bindings.length) bound.set(g, { object, bindings });
  }
  return { groups: bound, layout: device.createPipelineLayout({ bindGroupLayouts: layouts }) };
}

const gpuFloatView = new DataView(new ArrayBuffer(4));
function gpuFloat(bits) {
  gpuFloatView.setInt32(0, bits, true);
  return gpuFloatView.getFloat32(0, true);
}

function gpuBindingResource(record, kind, name) {
  switch (kind) {
    case GPU_ARG.BUFFER:
      if (record.type === "buffer") return { buffer: record.obj };
      break;
    case GPU_ARG.TEXTURE:
    case GPU_ARG.STORAGE_TEXTURE:
      if (record.type === "texture") return record.obj.createView();
      break;
    case GPU_ARG.SAMPLER:
      if (record.type === "sampler") return record.obj;
      break;
  }
  throw new GpuFailure("EINVALID", "the resource bound to " + name + " is not of the kind the shader expects");
}

// Builds and sets one bind group per group of the current pipeline from the pass's Bind state.
function gpuApplyBindings(state) {
  const { dev, pipeline, bindings, data, resource } = state;
  for (const [group, { object, bindings: slots }] of pipeline.groups) {
    const entries = [];
    for (const [binding, slot] of slots) {
      if (slot.kind === GPU_ARG.VALUE) {
        const block = new Uint8Array(slot.blockSize);
        for (const { stage, index, param } of slot.slots) {
          const bound = bindings.get(stage + ":" + index);
          if (!bound) throw new GpuFailure("EVALIDATION", "shader parameter " + param.name + " was not bound");
          if (bound.kind !== GPU_ARG.VALUE) throw new GpuFailure("EVALIDATION", "shader parameter " + param.name + " takes a value");
          if (bound.a < 0 || bound.b < 0 || bound.a + bound.b > data.length) {
            throw new GpuFailure("EVALIDATION", "the value of " + param.name + " lies outside the submitted data");
          }
          if (bound.b > param.size) {
            throw new GpuFailure("EVALIDATION", "the value of " + param.name + " has " + bound.b + " bytes; the shader takes " + param.size);
          }
          block.set(data.subarray(bound.a, bound.a + bound.b), param.offset);
        }
        const buffer = dev.device.createBuffer({ size: block.length, usage: GPU_BUFFER.UNIFORM | GPU_BUFFER.COPY_DST });
        dev.device.queue.writeBuffer(buffer, 0, block);
        state.transient.push(buffer);
        entries.push({ binding, resource: { buffer } });
        continue;
      }
      const first = slot.slots.find(s => bindings.has(s.stage + ":" + s.index));
      if (!first) throw new GpuFailure("EVALIDATION", "shader parameter " + slot.slots[0].param.name + " was not bound");
      const bound = bindings.get(first.stage + ":" + first.index);
      if (bound.kind !== slot.kind) throw new GpuFailure("EVALIDATION", "shader parameter " + first.param.name + " was bound to the wrong kind of resource");
      entries.push({ binding, resource: gpuBindingResource(resource(bound.a), slot.kind, first.param.name) });
    }
    state.pass.setBindGroup(group, dev.device.createBindGroup({ layout: object, entries }));
  }
}

function gpuAttachmentView(state, handle, what) {
  const record = state.resource(handle);
  if (record.type !== "texture") throw new GpuFailure("EINVALID", "the " + what + " is not a texture");
  return record;
}

function gpuBeginRenderPass(state, [color, colorClear, r, g, b, a, depth, depthClear, depthValue]) {
  const target = gpuAttachmentView(state, color, "color target");
  const desc = {
    colorAttachments: [{
      view: target.obj.createView(),
      loadOp: colorClear ? "clear" : "load",
      storeOp: "store",
      clearValue: { r: gpuFloat(r), g: gpuFloat(g), b: gpuFloat(b), a: gpuFloat(a) },
    }],
  };
  if (depth) {
    const record = gpuAttachmentView(state, depth, "depth target");
    const attachment = {
      view: record.obj.createView(),
      depthLoadOp: depthClear ? "clear" : "load",
      depthStoreOp: "store",
      depthClearValue: gpuFloat(depthValue),
    };
    if (record.format === "depth24plus-stencil8") Object.assign(attachment, { stencilLoadOp: "clear", stencilStoreOp: "store" });
    desc.depthStencilAttachment = attachment;
  }
  return state.encoder.beginRenderPass(desc);
}

// Operand counts per GpuOp.
const GPU_OPERANDS = [-1, 9, 0, 0, 1, 5, 3, 3, 3, 6, 4, 4, 5, 5, 3];

/**
 * Records one submit's command stream and returns the finished command buffer.
 * `resource(handle)` returns the live record for a handle of this device or throws. Per-submit
 * uniform buffers are pushed onto `transient`, even when recording fails, for the caller to
 * destroy once the GPU is done.
 */
export function gpuRecordCommands(dev, words, data, resource, pipelineOf, transient) {
  const encoder = dev.device.createCommandEncoder();
  const state = { dev, encoder, data, resource, pass: null, passKind: "", pipeline: null, bindings: new Map(), transient };
  const inPass = (kind, what) => {
    if (state.passKind !== kind) throw new GpuFailure("EVALIDATION", what + " was recorded outside a " + kind + " pass");
  };
  const ready = () => {
    if (!state.pipeline) throw new GpuFailure("EVALIDATION", "a draw or dispatch was recorded before set_pipeline");
    gpuApplyBindings(state);
  };
  for (let at = 0; at < words.length;) {
    const op = words[at++];
    const count = GPU_OPERANDS[op];
    if (count === undefined || count < 0) throw new GpuFailure("EVALIDATION", "unknown GPU command " + op);
    if (at + count > words.length) throw new GpuFailure("EVALIDATION", "the GPU command stream is truncated");
    const args = words.slice(at, at + count);
    at += count;
    switch (op) {
      case 1:
      case 2:
        if (state.pass) throw new GpuFailure("EVALIDATION", "a pass was begun before the previous one ended");
        state.pass = op === 1 ? gpuBeginRenderPass(state, args) : encoder.beginComputePass();
        state.passKind = op === 1 ? "render" : "compute";
        state.pipeline = null;
        state.bindings = new Map();
        break;
      case 3:
        if (!state.pass) throw new GpuFailure("EVALIDATION", "end_pass was recorded outside a pass");
        state.pass.end();
        state.pass = null;
        state.passKind = "";
        break;
      case 4: {
        if (!state.pass) throw new GpuFailure("EVALIDATION", "set_pipeline was recorded outside a pass");
        const pipeline = pipelineOf(args[0]);
        if (pipeline.kind !== state.passKind) throw new GpuFailure("EVALIDATION", "a " + pipeline.kind + " pipeline was set in a " + state.passKind + " pass");
        state.pipeline = pipeline;
        state.pass.setPipeline(pipeline.obj);
        break;
      }
      case 5:
        if (!state.pass) throw new GpuFailure("EVALIDATION", "bind was recorded outside a pass");
        state.bindings.set(args[0] + ":" + args[1], { kind: args[2], a: args[3], b: args[4] });
        break;
      case 6:
        inPass("compute", "dispatch");
        ready();
        state.pass.dispatchWorkgroups(args[0], args[1], args[2]);
        break;
      case 7: {
        inPass("render", "set_vertex_buffer");
        const record = resource(args[1]);
        if (record.type !== "buffer") throw new GpuFailure("EINVALID", "the vertex buffer is not a buffer");
        state.pass.setVertexBuffer(args[0], record.obj, args[2]);
        break;
      }
      case 8: {
        inPass("render", "set_index_buffer");
        const record = resource(args[0]);
        if (record.type !== "buffer") throw new GpuFailure("EINVALID", "the index buffer is not a buffer");
        state.pass.setIndexBuffer(record.obj, args[1] === 0 ? "uint16" : "uint32", args[2]);
        break;
      }
      case 9:
        inPass("render", "set_viewport");
        state.pass.setViewport(...Array.from(args, gpuFloat));
        break;
      case 10:
        inPass("render", "set_scissor");
        state.pass.setScissorRect(...args);
        break;
      case 11:
        inPass("render", "draw");
        ready();
        state.pass.draw(...args);
        break;
      case 12:
        inPass("render", "draw_indexed");
        ready();
        state.pass.drawIndexed(...args);
        break;
      case 13: {
        if (state.pass) throw new GpuFailure("EVALIDATION", "copy_buffer was recorded inside a pass");
        const [source, sourceOffset, destination, destinationOffset, size] = args;
        const from = resource(source);
        const to = resource(destination);
        if (from.type !== "buffer" || to.type !== "buffer") throw new GpuFailure("EINVALID", "copy_buffer needs two buffers");
        if ((sourceOffset | destinationOffset | size) & 3) {
          throw new GpuFailure("EINVALID", "WebGPU copies whole 4-byte words; the copy offsets and size must be multiples of 4");
        }
        encoder.copyBufferToBuffer(from.obj, sourceOffset, to.obj, destinationOffset, size);
        break;
      }
      case 14:
        throw new GpuFailure("EUNSUPPORTED", GPU_FEATURE.RAY_TRACING);
    }
  }
  if (state.pass) throw new GpuFailure("EVALIDATION", "a pass was not ended before submit");
  return encoder.finish();
}
