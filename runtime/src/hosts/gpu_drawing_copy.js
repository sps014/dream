export function drawingGpuCopy(record, source, width, height) {
  const dev = record.dev;
  if (record.surface || record.width !== width || record.height !== height ||
      record.obj.sampleCount !== 1 || !(record.obj.usage & 2)) {
    throw new Error("EINVAL\ndrawing copies require a matching single-sample texture with CopyDst usage");
  }
  const rgba = record.format === "rgba8unorm" || record.format === "rgba8unorm-srgb";
  const bgra = record.format === "bgra8unorm" || record.format === "bgra8unorm-srgb";
  if (!rgba && !bgra) throw new Error("EINVAL\ndrawing copies require an RGBA8 or BGRA8 texture");
  const encoder = dev.device.createCommandEncoder();
  if (bgra) {
    let stage = dev.drawingStage;
    if (!stage || stage.width !== width || stage.height !== height) {
      stage?.texture.destroy();
      const texture = dev.device.createTexture({
        label: "Dream drawing BGRA copy", size: [width, height],
        format: "bgra8unorm", usage: 16 | 1,
      });
      stage = dev.drawingStage = { width, height, texture };
    }
    if (!dev.drawingPipeline) {
      const module = dev.device.createShaderModule({ code: `
        @group(0) @binding(0) var image: texture_2d<f32>;
        @vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
          let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
          let y = f32(i & 2u) * 2.0 - 1.0;
          return vec4<f32>(x, y, 0.0, 1.0);
        }
        @fragment fn fragment(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
          return textureLoad(image, vec2<i32>(p.xy), 0);
        }` });
      dev.drawingPipeline = dev.device.createRenderPipeline({
        label: "Dream drawing BGRA conversion", layout: "auto",
        vertex: { module, entryPoint: "vertex" },
        fragment: { module, entryPoint: "fragment", targets: [{ format: "bgra8unorm" }] },
      });
    }
    if (stage.source !== source) {
      stage.source = source;
      stage.bind = dev.device.createBindGroup({
        layout: dev.drawingPipeline.getBindGroupLayout(0),
        entries: [{ binding: 0, resource: source.createView() }],
      });
    }
    const pass = encoder.beginRenderPass({ colorAttachments: [{
      view: stage.texture.createView(), loadOp: "clear", storeOp: "store", clearValue: [0, 0, 0, 0],
    }] });
    pass.setPipeline(dev.drawingPipeline);
    pass.setBindGroup(0, stage.bind);
    pass.draw(3);
    pass.end();
    source = stage.texture;
  }
  encoder.copyTextureToTexture({ texture: source }, { texture: record.obj }, [width, height, 1]);
  dev.device.queue.submit([encoder.finish()]);
}
