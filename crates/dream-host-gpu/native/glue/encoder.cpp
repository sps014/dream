#include "glue/internal.h"

#include <slang-rhi/shader-cursor.h>

using namespace rhi;
using namespace dream::gpu;

namespace {

int passError(const char* what)
{
    return fail(DGPU_INVALID, what);
}

void endPass(DgpuEncoder* e)
{
    if (e->render)
        e->render->end();
    if (e->compute)
        e->compute->end();
    if (e->rayTracing)
        e->rayTracing->end();
    e->render = nullptr;
    e->compute = nullptr;
    e->rayTracing = nullptr;
    e->pipeline = nullptr;
    e->root = nullptr;
}

Extent3D mipExtent(ITexture* texture, uint32_t mip)
{
    const TextureDesc& desc = texture->getDesc();
    auto at = [mip](uint32_t size) { return std::max<uint32_t>(1, size >> mip); };
    return {at(desc.size.width), at(desc.size.height), 1};
}

} // namespace

extern "C" int dgpu_encoder_begin(DgpuDevice* device, DgpuEncoder** out)
{
    DeviceScope deviceScope(device->device);
    *out = nullptr;
    auto encoder = new DgpuEncoder();
    encoder->device = device;
    Result result = device->queue->createCommandEncoder(encoder->encoder.writeRef());
    if (SLANG_FAILED(result) || !encoder->encoder)
    {
        delete encoder;
        return failResult(device, result, "command encoder creation failed");
    }
    *out = encoder;
    return DGPU_OK;
}

extern "C" void dgpu_encoder_abort(DgpuEncoder* e)
{
    if (!e)
        return;
    DeviceScope deviceScope(e->device->device);
    endPass(e);
    delete e;
}

extern "C" int dgpu_encoder_submit(DgpuEncoder* e, uint64_t* fence_value)
{
    DeviceScope deviceScope(e->device->device);
    std::unique_ptr<DgpuEncoder> owned(e);
    DgpuDevice* device = e->device;
    endPass(e);
    ComPtr<ICommandBuffer> commands;
    Result result = e->encoder->finish(commands.writeRef());
    if (SLANG_FAILED(result))
        return failResult(device, result, "finishing the command buffer failed");
    uint64_t value = ++device->nextFenceValue;
    ICommandBuffer* buffers[] = {commands};
    IFence* fences[] = {device->fence};
    SubmitDesc submit;
    submit.commandBuffers = buffers;
    submit.commandBufferCount = 1;
    submit.signalFences = fences;
    submit.signalFenceValues = &value;
    submit.signalFenceCount = 1;
    result = device->queue->submit(submit);
    if (SLANG_FAILED(result))
        return failResult(device, result, "submitting commands failed");
    *fence_value = value;
    return DGPU_OK;
}

extern "C" int dgpu_enc_write_buffer(DgpuEncoder* e, DgpuObject* buffer, uint64_t offset, const void* data, uint64_t size)
{
    DeviceScope deviceScope(e->device->device);
    Result result = e->encoder->uploadBufferData(static_cast<IBuffer*>(buffer), offset, size, data);
    return SLANG_FAILED(result) ? failResult(e->device, result, "buffer upload failed") : DGPU_OK;
}

extern "C" int dgpu_enc_write_texture(
    DgpuEncoder* e,
    DgpuObject* texture,
    uint32_t mip,
    uint32_t layer,
    const void* data,
    uint64_t size
)
{
    DeviceScope deviceScope(e->device->device);
    ITexture* t = static_cast<ITexture*>(texture);
    SubresourceLayout tight;
    if (SLANG_FAILED(t->getSubresourceLayout(mip, 1, &tight)))
        return fail(DGPU_INVALID, "the texture has no such mip level");
    if (size != tight.sizeInBytes)
        return fail(DGPU_INVALID, "texture data size does not match the mip level");
    SubresourceData subresource = {data, tight.rowPitch, tight.slicePitch};
    Result result = e->encoder->uploadTextureData(t, {layer, 1, mip, 1}, {0, 0, 0}, mipExtent(t, mip), &subresource, 1);
    return SLANG_FAILED(result) ? failResult(e->device, result, "texture upload failed") : DGPU_OK;
}

extern "C" int dgpu_enc_copy_buffer(
    DgpuEncoder* e,
    DgpuObject* dst,
    uint64_t dst_offset,
    DgpuObject* src,
    uint64_t src_offset,
    uint64_t size
)
{
    DeviceScope deviceScope(e->device->device);
    e->encoder->copyBuffer(static_cast<IBuffer*>(dst), dst_offset, static_cast<IBuffer*>(src), src_offset, size);
    return DGPU_OK;
}

extern "C" int dgpu_enc_read_texture(DgpuEncoder* e, DgpuObject* texture, uint32_t mip, uint32_t layer, DgpuObject* readback)
{
    DeviceScope deviceScope(e->device->device);
    ITexture* t = static_cast<ITexture*>(texture);
    SubresourceLayout layout;
    if (SLANG_FAILED(t->getSubresourceLayout(mip, &layout)))
        return fail(DGPU_INVALID, "the texture has no such mip level");
    IBuffer* staging = static_cast<IBuffer*>(readback);
    e->encoder->copyTextureToBuffer(
        staging,
        0,
        staging->getDesc().size,
        layout.rowPitch,
        t,
        layer,
        mip,
        {0, 0, 0},
        mipExtent(t, mip)
    );
    return DGPU_OK;
}

extern "C" int dgpu_enc_begin_render_pass(
    DgpuEncoder* e,
    DgpuObject* color,
    int clear_color,
    const float* rgba,
    DgpuObject* depth,
    int clear_depth,
    float depth_value
)
{
    DeviceScope deviceScope(e->device->device);
    if (e->render || e->compute || e->rayTracing)
        return passError("a pass is already open");
    ITexture* target = static_cast<ITexture*>(color);
    RenderPassColorAttachment colorAttachment;
    colorAttachment.view = target->getDefaultView();
    colorAttachment.loadOp = clear_color ? LoadOp::Clear : LoadOp::Load;
    for (int i = 0; i < 4; i++)
        colorAttachment.clearValue[i] = rgba[i];
    RenderPassDepthStencilAttachment depthAttachment;
    RenderPassDesc desc;
    desc.colorAttachments = &colorAttachment;
    desc.colorAttachmentCount = 1;
    if (depth)
    {
        depthAttachment.view = static_cast<ITexture*>(depth)->getDefaultView();
        depthAttachment.depthLoadOp = clear_depth ? LoadOp::Clear : LoadOp::Load;
        depthAttachment.depthClearValue = depth_value;
        depthAttachment.stencilLoadOp = LoadOp::Clear;
        desc.depthStencilAttachment = &depthAttachment;
    }
    e->render = e->encoder->beginRenderPass(desc);
    if (!e->render)
        return failResult(e->device, SLANG_FAIL, "beginning a render pass failed");
    e->state = RenderState();
    e->viewportSet = false;
    e->scissorSet = false;
    e->targetWidth = target->getDesc().size.width;
    e->targetHeight = target->getDesc().size.height;
    e->pipeline = nullptr;
    e->root = nullptr;
    return DGPU_OK;
}

extern "C" int dgpu_enc_begin_compute_pass(DgpuEncoder* e)
{
    DeviceScope deviceScope(e->device->device);
    if (e->render || e->compute || e->rayTracing)
        return passError("a pass is already open");
    e->compute = e->encoder->beginComputePass();
    if (!e->compute)
        return failResult(e->device, SLANG_FAIL, "beginning a compute pass failed");
    e->pipeline = nullptr;
    e->root = nullptr;
    return DGPU_OK;
}

extern "C" int dgpu_enc_end_pass(DgpuEncoder* e)
{
    DeviceScope deviceScope(e->device->device);
    if (!e->render && !e->compute && !e->rayTracing)
        return passError("no pass is open");
    endPass(e);
    return DGPU_OK;
}

extern "C" int dgpu_enc_set_pipeline(DgpuEncoder* e, DgpuPipeline* pipeline)
{
    DeviceScope deviceScope(e->device->device);
    if (pipeline->rayTracing)
    {
        // The command stream traces rays inside a compute pass; slang-rhi records them in a
        // ray tracing pass of their own, which replaces the compute pass from here on.
        if (!e->compute && !e->rayTracing)
            return passError("ray tracing pipelines need a compute pass");
        if (e->compute)
        {
            e->compute->end();
            e->compute = nullptr;
            e->rayTracing = e->encoder->beginRayTracingPass();
            if (!e->rayTracing)
                return failResult(e->device, SLANG_FAIL, "beginning a ray tracing pass failed");
        }
        e->root = e->rayTracing->bindPipeline(pipeline->rayTracingPipeline, pipeline->shaderTable);
    }
    else if (pipeline->render ? !e->render : !e->compute)
        return passError(pipeline->render ? "render pipelines need a render pass" : "compute pipelines need a compute pass");
    else
        e->root = pipeline->render ? e->render->bindPipeline(pipeline->graphics) : e->compute->bindPipeline(pipeline->compute);
    if (!e->root)
        return failResult(e->device, SLANG_FAIL, "binding the pipeline failed");
    e->pipeline = pipeline;
    return e->device->backend == 6 ? fillUnboundSlots(e->device, e->root) : DGPU_OK;
}

extern "C" int dgpu_enc_bind(DgpuEncoder* e, uint32_t entry, uint32_t index, DgpuObject* resource, const void* data, uint32_t size)
{
    DeviceScope deviceScope(e->device->device);
    if (!e->pipeline || !e->root)
        return passError("bind needs a pipeline");
    if (entry >= e->pipeline->stages.size() || index >= e->pipeline->stages[entry].size())
        return passError("the pipeline has no such shader parameter");
    const BindingPath& path = e->pipeline->stages[entry][index];
    IShaderObject* object = path.object < 0 ? e->root : e->root->getEntryPoint(uint32_t(path.object));
    if (!object)
        return passError("the pipeline has no such entry point object");
    ShaderCursor cursor(object);
    if (!path.field.empty())
        cursor = cursor.getField(path.field.c_str());
    if (cursor.isValid() && path.dereference)
        cursor = cursor.getDereferenced();
    if (!cursor.isValid())
        return passError("the shader parameter's binding is not addressable");
    cursor.m_offset.uniformOffset += path.uniformOffset;
    Result result;
    switch (path.kind)
    {
    case 1:
    case 7:
        result = cursor.setBinding(Binding(static_cast<IBuffer*>(resource)));
        break;
    case 2:
    case 4:
        result = cursor.setBinding(Binding(static_cast<ITexture*>(resource)));
        break;
    case 3:
        result = cursor.setBinding(Binding(static_cast<ISampler*>(resource)));
        break;
    case 5:
        result = cursor.setBinding(Binding(static_cast<IAccelerationStructure*>(resource)));
        break;
    case 6:
        result = cursor.setData(data, size);
        break;
    default:
        return fail(DGPU_UNSUPPORTED, "unsupported binding kind");
    }
    return SLANG_FAILED(result) ? failResult(e->device, result, "binding a shader parameter failed") : DGPU_OK;
}

extern "C" int dgpu_enc_set_vertex_buffer(DgpuEncoder* e, uint32_t slot, DgpuObject* buffer, uint64_t offset)
{
    DeviceScope deviceScope(e->device->device);
    if (!e->render || slot >= 16)
        return passError("vertex buffers need a render pass and a slot below 16");
    e->state.vertexBuffers[slot] = {static_cast<IBuffer*>(buffer), offset};
    e->state.vertexBufferCount = std::max(e->state.vertexBufferCount, slot + 1);
    return DGPU_OK;
}

extern "C" int dgpu_enc_set_index_buffer(DgpuEncoder* e, DgpuObject* buffer, uint32_t format, uint64_t offset)
{
    DeviceScope deviceScope(e->device->device);
    if (!e->render)
        return passError("index buffers need a render pass");
    e->state.indexBuffer = {static_cast<IBuffer*>(buffer), offset};
    e->state.indexFormat = format == 0 ? IndexFormat::Uint16 : IndexFormat::Uint32;
    return DGPU_OK;
}

extern "C" int dgpu_enc_set_viewport(DgpuEncoder* e, const float* v)
{
    DeviceScope deviceScope(e->device->device);
    if (!e->render)
        return passError("viewports need a render pass");
    e->state.viewports[0] = {v[0], v[1], v[2], v[3], v[4], v[5]};
    e->state.viewportCount = 1;
    e->viewportSet = true;
    return DGPU_OK;
}

extern "C" int dgpu_enc_set_scissor(DgpuEncoder* e, int32_t x, int32_t y, int32_t width, int32_t height)
{
    DeviceScope deviceScope(e->device->device);
    if (!e->render)
        return passError("scissors need a render pass");
    e->state.scissorRects[0] = {uint32_t(x), uint32_t(y), uint32_t(x + width), uint32_t(y + height)};
    e->state.scissorRectCount = 1;
    e->scissorSet = true;
    return DGPU_OK;
}

namespace {

int prepareDraw(DgpuEncoder* e)
{
    if (!e->render || !e->pipeline || !e->pipeline->render)
        return passError("draws need a render pass with a render pipeline");
    // Without explicit state the whole target is drawn, as in WebGPU.
    if (!e->viewportSet)
    {
        e->state.viewports[0] = Viewport::fromSize(float(e->targetWidth), float(e->targetHeight));
        e->state.viewportCount = 1;
    }
    if (!e->scissorSet)
    {
        e->state.scissorRects[0] = ScissorRect::fromSize(e->targetWidth, e->targetHeight);
        e->state.scissorRectCount = 1;
    }
    e->render->setRenderState(e->state);
    return DGPU_OK;
}

} // namespace

extern "C" int dgpu_enc_draw(DgpuEncoder* e, uint32_t vertices, uint32_t instances, uint32_t first_vertex, uint32_t first_instance)
{
    DeviceScope deviceScope(e->device->device);
    int status = prepareDraw(e);
    if (status != DGPU_OK)
        return status;
    DrawArguments args;
    args.vertexCount = vertices;
    args.instanceCount = instances;
    args.startVertexLocation = first_vertex;
    args.startInstanceLocation = first_instance;
    e->render->draw(args);
    return DGPU_OK;
}

extern "C" int dgpu_enc_draw_indexed(
    DgpuEncoder* e,
    uint32_t indices,
    uint32_t instances,
    uint32_t first_index,
    int32_t base_vertex,
    uint32_t first_instance
)
{
    DeviceScope deviceScope(e->device->device);
    int status = prepareDraw(e);
    if (status != DGPU_OK)
        return status;
    if (!e->state.indexBuffer.buffer)
        return passError("draw_indexed needs an index buffer");
    DrawArguments args;
    args.vertexCount = indices;
    args.instanceCount = instances;
    args.startIndexLocation = first_index;
    args.startVertexLocation = uint32_t(base_vertex);
    args.startInstanceLocation = first_instance;
    e->render->drawIndexed(args);
    return DGPU_OK;
}

extern "C" int dgpu_enc_dispatch(DgpuEncoder* e, uint32_t x, uint32_t y, uint32_t z)
{
    DeviceScope deviceScope(e->device->device);
    if (!e->compute || !e->pipeline || e->pipeline->render)
        return passError("dispatch needs a compute pass with a compute pipeline");
    e->compute->dispatchCompute(x, y, z);
    return DGPU_OK;
}

extern "C" int dgpu_enc_trace_rays(DgpuEncoder* e, uint32_t width, uint32_t height, uint32_t depth)
{
    DeviceScope deviceScope(e->device->device);
    if (!e->rayTracing || !e->pipeline || !e->pipeline->rayTracing)
        return passError("trace_rays needs a ray tracing pipeline");
    e->rayTracing->dispatchRays(0, width, height, depth);
    return DGPU_OK;
}
