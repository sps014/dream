#include "glue/internal.h"

#include "replay/replay.h"

using namespace rhi;

namespace dream::gpu {
ComparisonFunc compare(uint32_t value);
}

namespace {

Format vertexFormat(uint32_t format)
{
    // GpuVertexFormat order.
    static const Format kFormats[] = {
        Format::R32Float,
        Format::RG32Float,
        Format::RGB32Float,
        Format::RGBA32Float,
        Format::R32Sint,
        Format::R32Uint,
        Format::RGBA8Unorm,
    };
    return format < std::size(kFormats) ? kFormats[format] : Format::Undefined;
}

PrimitiveTopology topology(uint32_t value)
{
    // GpuTopology order.
    static const PrimitiveTopology kTopologies[] = {
        PrimitiveTopology::TriangleList,
        PrimitiveTopology::TriangleStrip,
        PrimitiveTopology::LineList,
        PrimitiveTopology::LineStrip,
        PrimitiveTopology::PointList,
    };
    return value < std::size(kTopologies) ? kTopologies[value] : PrimitiveTopology::TriangleList;
}

void blend(ColorTargetDesc& target, uint32_t mode)
{
    // GpuBlendMode: Replace, Alpha, Premultiplied, Additive.
    if (mode == 0)
        return;
    target.enableBlend = true;
    BlendFactor src = mode == 1 ? BlendFactor::SrcAlpha : BlendFactor::One;
    BlendFactor dst = mode == 3 ? BlendFactor::One : BlendFactor::InvSrcAlpha;
    target.color = {src, dst, BlendOp::Add};
    target.alpha = {BlendFactor::One, dst, BlendOp::Add};
}

} // namespace

using namespace dream::gpu;

extern "C" int dgpu_program_create(
    DgpuDevice* device,
    const void* layout,
    size_t layout_size,
    const DgpuEntryCode* entries,
    uint32_t entry_count,
    DgpuProgram** out
)
{
    *out = nullptr;
    std::vector<dream::slang_replay::PrecompiledEntryPoint> code;
    for (uint32_t i = 0; i < entry_count; i++)
        code.push_back({entries[i].name, entries[i].code, entries[i].size});
    ComPtr<slang::IComponentType> component;
    std::string error;
    if (SLANG_FAILED(dream::slang_replay::createPrecompiledProgram(
            layout,
            layout_size,
            code.data(),
            entry_count,
            component.writeRef(),
            &error
        )))
        return fail(DGPU_INVALID, "the shader bundle's layout does not match its code: " + error);
    ShaderProgramDesc desc;
    desc.slangGlobalScope = component;
    ComPtr<IShaderProgram> program;
    Result result = device->device->createShaderProgram(desc, program.writeRef());
    if (SLANG_FAILED(result) || !program)
        return failResult(device, result, "shader program creation failed");
    auto p = new DgpuProgram();
    p->device = device;
    p->component = component;
    p->program = program;
    *out = p;
    return DGPU_OK;
}

extern "C" void dgpu_program_release(DgpuProgram* program)
{
    delete program;
}

extern "C" int dgpu_compute_pipeline_create(
    DgpuDevice* device,
    DgpuProgram* program,
    const DgpuStageParams* stage,
    DgpuPipeline** out
)
{
    *out = nullptr;
    auto pipeline = std::make_unique<DgpuPipeline>();
    int status = resolveBindings(program, stage, 1, pipeline->stages);
    if (status != DGPU_OK)
        return status;
    ComputePipelineDesc desc;
    desc.program = program->program;
    Result result = device->device->createComputePipeline(desc, pipeline->compute.writeRef());
    if (SLANG_FAILED(result) || !pipeline->compute)
        return failResult(device, result, "compute pipeline creation failed");
    *out = pipeline.release();
    return DGPU_OK;
}

extern "C" int dgpu_render_pipeline_create(
    DgpuDevice* device,
    DgpuProgram* program,
    const DgpuStageParams* stages,
    const DgpuRenderDesc* d,
    DgpuPipeline** out
)
{
    *out = nullptr;
    auto pipeline = std::make_unique<DgpuPipeline>();
    pipeline->render = true;
    int status = resolveBindings(program, stages, 2, pipeline->stages);
    if (status != DGPU_OK)
        return status;

    // Always an input layout, even an empty one: slang-rhi's WebGPU backend reads it
    // unconditionally.
    ComPtr<IInputLayout> inputLayout;
    {
        // The generator gives each `@location(n)` input the semantic LOCATION<n> (D3D binds by
        // semantic); the other backends take element i as location i, so the attributes arrive
        // sorted with dense locations.
        std::vector<InputElementDesc> elements;
        for (uint32_t i = 0; i < d->attribute_count; i++)
        {
            const DgpuVertexAttribute& a = d->attributes[i];
            Format format = vertexFormat(a.format);
            if (format == Format::Undefined)
                return fail(DGPU_INVALID, "unknown vertex format");
            elements.push_back({"LOCATION", a.location, format, a.offset, a.slot});
        }
        std::vector<VertexStreamDesc> streams;
        for (uint32_t i = 0; i < d->buffer_count; i++)
            streams.push_back(
                {d->buffers[i].stride,
                 d->buffers[i].per_instance ? InputSlotClass::PerInstance : InputSlotClass::PerVertex,
                 d->buffers[i].per_instance ? 1u : 0u}
            );
        InputLayoutDesc layoutDesc;
        layoutDesc.inputElements = elements.data();
        layoutDesc.inputElementCount = uint32_t(elements.size());
        layoutDesc.vertexStreams = streams.data();
        layoutDesc.vertexStreamCount = uint32_t(streams.size());
        Result result = device->device->createInputLayout(layoutDesc, inputLayout.writeRef());
        if (SLANG_FAILED(result))
            return failResult(device, result, "vertex layout creation failed");
    }

    Format color = textureFormat(d->color_format);
    if (color == Format::Undefined)
        return fail(DGPU_INVALID, "unknown color format");
    ColorTargetDesc target;
    target.format = color;
    blend(target, d->blend);

    RenderPipelineDesc desc;
    desc.program = program->program;
    desc.inputLayout = inputLayout;
    desc.primitiveTopology = topology(d->topology);
    desc.targets = &target;
    desc.targetCount = 1;
    if (d->depth_format > 0)
    {
        desc.depthStencil.format = textureFormat(d->depth_format - 1);
        desc.depthStencil.depthTestEnable = true;
        desc.depthStencil.depthWriteEnable = d->depth_write != 0;
        desc.depthStencil.depthFunc = compare(d->depth_compare);
    }
    desc.rasterizer.cullMode = d->cull == 1 ? CullMode::Front : d->cull == 2 ? CullMode::Back : CullMode::None;
    desc.multisample.sampleCount = d->samples > 1 ? d->samples : 1;
    Result result = device->device->createRenderPipeline(desc, pipeline->graphics.writeRef());
    if (SLANG_FAILED(result) || !pipeline->graphics)
        return failResult(device, result, "render pipeline creation failed");
    *out = pipeline.release();
    return DGPU_OK;
}

extern "C" void dgpu_pipeline_release(DgpuPipeline* pipeline)
{
    delete pipeline;
}
