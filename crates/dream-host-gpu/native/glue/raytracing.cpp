// Ray tracing over slang-rhi: acceleration structure builds, ray tracing pipelines with their
// shader binding tables. Requests arrive validated from src/raytracing.rs.

#include "glue/internal.h"

#include <slang-rhi/acceleration-structure-utils.h>

#include <cstring>

using namespace rhi;
using namespace dream::gpu;

namespace {

int buildInto(
    DgpuEncoder* e,
    const AccelerationStructureBuildInput& input,
    AccelerationStructureKind kind,
    DgpuObject** out,
    DgpuObject** scratch
)
{
    IDevice* device = e->device->device;
    AccelerationStructureBuildDesc build;
    build.inputs = &input;
    build.inputCount = 1;
    build.flags = AccelerationStructureBuildFlags::PreferFastTrace;
    AccelerationStructureSizes sizes;
    Result result = device->getAccelerationStructureSizes(build, &sizes);
    if (SLANG_FAILED(result))
        return failResult(e->device, result, "sizing the acceleration structure failed");

    BufferDesc scratchDesc;
    scratchDesc.size = std::max<uint64_t>(sizes.scratchSize, 4);
    scratchDesc.usage = BufferUsage::UnorderedAccess;
    scratchDesc.defaultState = ResourceState::UnorderedAccess;
    ComPtr<IBuffer> scratchBuffer;
    result = device->createBuffer(scratchDesc, nullptr, scratchBuffer.writeRef());
    if (SLANG_FAILED(result))
        return failResult(e->device, result, "allocating acceleration structure scratch memory failed");

    AccelerationStructureDesc desc;
    desc.kind = kind;
    desc.size = sizes.accelerationStructureSize;
    desc.flags = build.flags;
    ComPtr<IAccelerationStructure> structure;
    result = device->createAccelerationStructure(desc, structure.writeRef());
    if (SLANG_FAILED(result))
        return failResult(e->device, result, "creating the acceleration structure failed");

    e->encoder->buildAccelerationStructure(build, structure, nullptr, scratchBuffer, 0, nullptr);
    *out = structure.detach();
    *scratch = scratchBuffer.detach();
    return DGPU_OK;
}

} // namespace

extern "C" int dgpu_enc_build_bottom_level(
    DgpuEncoder* e,
    const DgpuGeometry* geometry,
    DgpuObject** out,
    DgpuObject** scratch
)
{
    DeviceScope deviceScope(e->device->device);
    *out = nullptr;
    *scratch = nullptr;
    AccelerationStructureGeometryFlags flags =
        geometry->opaque ? AccelerationStructureGeometryFlags::Opaque : AccelerationStructureGeometryFlags::None;
    AccelerationStructureBuildInput input = {};
    if (geometry->kind == 0)
    {
        input.type = AccelerationStructureBuildInputType::Triangles;
        input.triangles = AccelerationStructureBuildInputTriangles();
        input.triangles.vertexBuffers[0] = BufferOffsetPair(static_cast<IBuffer*>(geometry->vertices));
        input.triangles.vertexBufferCount = 1;
        input.triangles.vertexFormat = Format::RGB32Float;
        input.triangles.vertexCount = geometry->vertex_count;
        input.triangles.vertexStride = geometry->vertex_stride;
        if (geometry->indices)
        {
            input.triangles.indexBuffer = BufferOffsetPair(static_cast<IBuffer*>(geometry->indices));
            input.triangles.indexFormat = IndexFormat::Uint32;
            input.triangles.indexCount = geometry->index_count;
        }
        input.triangles.flags = flags;
    }
    else
    {
        input.type = AccelerationStructureBuildInputType::ProceduralPrimitives;
        input.proceduralPrimitives = AccelerationStructureBuildInputProceduralPrimitives();
        input.proceduralPrimitives.aabbBuffers[0] = BufferOffsetPair(static_cast<IBuffer*>(geometry->vertices));
        input.proceduralPrimitives.aabbBufferCount = 1;
        input.proceduralPrimitives.aabbStride = 2 * geometry->vertex_stride;
        input.proceduralPrimitives.primitiveCount = geometry->vertex_count / 2;
        input.proceduralPrimitives.flags = flags;
    }
    return buildInto(e, input, AccelerationStructureKind::BottomLevel, out, scratch);
}

extern "C" int dgpu_enc_build_top_level(
    DgpuEncoder* e,
    const DgpuInstance* instances,
    uint32_t count,
    DgpuObject** out,
    DgpuObject** scratch,
    DgpuObject** instance_buffer
)
{
    DeviceScope deviceScope(e->device->device);
    *out = nullptr;
    *scratch = nullptr;
    *instance_buffer = nullptr;
    IDevice* device = e->device->device;
    AccelerationStructureInstanceDescType type = getAccelerationStructureInstanceDescType(device);
    size_t stride = getAccelerationStructureInstanceDescSize(type);
    std::vector<uint8_t> records(std::max<size_t>(1, count) * stride);
    for (uint32_t i = 0; i < count; i++)
    {
        const DgpuInstance& in = instances[i];
        AccelerationStructureInstanceDescGeneric generic = {};
        std::memcpy(generic.transform, in.transform, sizeof(generic.transform));
        generic.instanceID = in.id & 0xFFFFFF;
        generic.instanceMask = in.mask & 0xFF;
        generic.instanceContributionToHitGroupIndex = in.hit_group_offset & 0xFFFFFF;
        generic.flags = AccelerationStructureInstanceFlags(in.flags & 0xFF);
        generic.accelerationStructure = static_cast<IAccelerationStructure*>(in.bottom_level)->getHandle();
        convertAccelerationStructureInstanceDesc(type, records.data() + i * stride, &generic);
    }
    BufferDesc desc;
    desc.size = records.size();
    desc.usage = BufferUsage::AccelerationStructureBuildInput | BufferUsage::ShaderResource;
    desc.defaultState = ResourceState::AccelerationStructureBuildInput;
    ComPtr<IBuffer> buffer;
    Result result = device->createBuffer(desc, records.data(), buffer.writeRef());
    if (SLANG_FAILED(result))
        return failResult(e->device, result, "uploading the acceleration structure instances failed");

    AccelerationStructureBuildInput input = {};
    input.type = AccelerationStructureBuildInputType::Instances;
    input.instances.instanceBuffer = BufferOffsetPair(buffer);
    input.instances.instanceStride = uint32_t(stride);
    input.instances.instanceCount = count;
    int status = buildInto(e, input, AccelerationStructureKind::TopLevel, out, scratch);
    if (status == DGPU_OK)
        *instance_buffer = buffer.detach();
    return status;
}

extern "C" void dgpu_object_retain(DgpuObject* object)
{
    if (object)
        static_cast<ISlangUnknown*>(object)->addRef();
}

extern "C" int dgpu_ray_tracing_pipeline_create(
    DgpuDevice* device,
    DgpuProgram* program,
    const DgpuStageParams* stages,
    uint32_t stage_count,
    const DgpuRayTracingDesc* d,
    DgpuPipeline** out
)
{
    DeviceScope deviceScope(device->device);
    *out = nullptr;
    auto pipeline = std::make_unique<DgpuPipeline>();
    pipeline->rayTracing = true;
    int status = resolveBindings(program, stages, stage_count, pipeline->stages);
    if (status != DGPU_OK)
        return status;

    std::vector<std::string> names;
    std::vector<HitGroupDesc> groups;
    for (uint32_t i = 0; i < d->hit_group_count; i++)
        names.push_back("hit_group_" + std::to_string(i));
    for (uint32_t i = 0; i < d->hit_group_count; i++)
    {
        HitGroupDesc group;
        group.hitGroupName = names[i].c_str();
        group.closestHitEntryPoint = d->hit_groups[i].closest_hit;
        group.anyHitEntryPoint = d->hit_groups[i].any_hit;
        group.intersectionEntryPoint = d->hit_groups[i].intersection;
        groups.push_back(group);
    }
    RayTracingPipelineDesc desc;
    desc.program = program->program;
    desc.hitGroupCount = uint32_t(groups.size());
    desc.hitGroups = groups.data();
    desc.maxRecursion = d->max_recursion;
    desc.maxRayPayloadSize = d->max_payload_size;
    desc.maxAttributeSizeInBytes = d->max_attribute_size;
    Result result = device->device->createRayTracingPipeline(desc, pipeline->rayTracingPipeline.writeRef());
    if (SLANG_FAILED(result) || !pipeline->rayTracingPipeline)
        return failResult(device, result, "ray tracing pipeline creation failed");

    std::vector<const char*> groupNames;
    for (const std::string& name : names)
        groupNames.push_back(name.c_str());
    ShaderTableDesc table;
    table.program = program->program;
    table.rayGenShaderCount = 1;
    table.rayGenShaderEntryPointNames = &d->ray_generation;
    table.missShaderCount = d->miss_count;
    table.missShaderEntryPointNames = d->misses;
    table.hitGroupCount = uint32_t(groupNames.size());
    table.hitGroupNames = groupNames.data();
    table.callableShaderCount = d->callable_count;
    table.callableShaderEntryPointNames = d->callables;
    result = device->device->createShaderTable(table, pipeline->shaderTable.writeRef());
    if (SLANG_FAILED(result) || !pipeline->shaderTable)
        return failResult(device, result, "shader binding table creation failed");
    *out = pipeline.release();
    return DGPU_OK;
}
