#include "glue/internal.h"

#include <cstring>

using namespace rhi;

namespace dream::gpu {

namespace {

// GpuTextureFormat order.
const Format kTextureFormats[] = {
    Format::RGBA8Unorm,
    Format::RGBA8UnormSrgb,
    Format::BGRA8Unorm,
    Format::BGRA8UnormSrgb,
    Format::R8Unorm,
    Format::R32Float,
    Format::RG32Float,
    Format::RGBA16Float,
    Format::RGBA32Float,
    Format::R32Uint,
    Format::D32Float,
    // WebGPU's depth24plus-stencil8 promises at least 24 depth bits; every backend has D32+S8.
    Format::D32FloatS8Uint,
};

bool isDepth(Format format)
{
    return format == Format::D32Float || format == Format::D32FloatS8Uint;
}

TextureFilteringMode filter(int32_t value)
{
    return value == 0 ? TextureFilteringMode::Point : TextureFilteringMode::Linear;
}

TextureAddressingMode address(int32_t value)
{
    switch (value)
    {
    case 0:
        return TextureAddressingMode::ClampToEdge;
    case 2:
        return TextureAddressingMode::MirrorRepeat;
    default:
        return TextureAddressingMode::Wrap;
    }
}

} // namespace

Format textureFormat(uint32_t format)
{
    return format < std::size(kTextureFormats) ? kTextureFormats[format] : Format::Undefined;
}

uint32_t dreamTextureFormat(Format format)
{
    for (uint32_t i = 0; i < std::size(kTextureFormats); i++)
        if (kTextureFormats[i] == format)
            return i;
    return UINT32_MAX;
}

ComparisonFunc compare(uint32_t value)
{
    // GpuCompare order: Never, Less, LessEqual, Equal, GreaterEqual, Greater, NotEqual, Always.
    static const ComparisonFunc kCompare[] = {
        ComparisonFunc::Never,
        ComparisonFunc::Less,
        ComparisonFunc::LessEqual,
        ComparisonFunc::Equal,
        ComparisonFunc::GreaterEqual,
        ComparisonFunc::Greater,
        ComparisonFunc::NotEqual,
        ComparisonFunc::Always,
    };
    return value < std::size(kCompare) ? kCompare[value] : ComparisonFunc::Always;
}

} // namespace dream::gpu

using namespace dream::gpu;

extern "C" int dgpu_buffer_create(DgpuDevice* device, uint64_t size, uint32_t usage, int readback, DgpuObject** out)
{
    DeviceScope deviceScope(device->device);
    *out = nullptr;
    BufferDesc desc;
    // Zero-length buffers are valid in Dream; backends reject them.
    desc.size = size == 0 ? 4 : size;
    if (readback)
    {
        desc.memoryType = MemoryType::ReadBack;
        desc.usage = BufferUsage::CopyDestination;
        desc.defaultState = ResourceState::CopyDestination;
    }
    else
    {
        desc.usage = BufferUsage::ShaderResource | BufferUsage::UnorderedAccess | BufferUsage::CopySource |
                     BufferUsage::CopyDestination;
        if (usage & 2)
            desc.usage |= BufferUsage::VertexBuffer;
        if (usage & 4)
            desc.usage |= BufferUsage::IndexBuffer;
        if (usage & 8)
            desc.usage |= BufferUsage::IndirectArgument;
        // Any buffer may hold the vertices, indices or boxes of an acceleration structure build.
        if (device->device->hasFeature(Feature::AccelerationStructure))
            desc.usage |= BufferUsage::AccelerationStructureBuildInput;
        desc.defaultState = ResourceState::UnorderedAccess;
    }
    ComPtr<IBuffer> buffer;
    Result result = device->device->createBuffer(desc, nullptr, buffer.writeRef());
    if (SLANG_FAILED(result) || !buffer)
        return failResult(device, result, "buffer creation failed");
    if (!readback)
    {
        // New buffers read as zero, as Dream promises; not every backend clears allocations.
        ComPtr<ICommandEncoder> encoder = device->queue->createCommandEncoder();
        encoder->clearBuffer(buffer);
        result = device->queue->submit(encoder->finish());
        if (SLANG_FAILED(result))
            return failResult(device, result, "clearing a new buffer failed");
    }
    *out = buffer.detach();
    return DGPU_OK;
}

extern "C" int dgpu_buffer_read_mapped(
    DgpuDevice* device,
    DgpuObject* buffer,
    void* out,
    uint64_t row_pitch,
    uint64_t row_bytes,
    uint32_t rows
)
{
    DeviceScope deviceScope(device->device);
    IBuffer* staging = static_cast<IBuffer*>(buffer);
    void* mapped = nullptr;
    Result result = device->device->mapBuffer(staging, CpuAccessMode::Read, &mapped);
    if (SLANG_FAILED(result) || !mapped)
        return failResult(device, result, "mapping a readback buffer failed");
    auto dst = static_cast<uint8_t*>(out);
    auto src = static_cast<const uint8_t*>(mapped);
    for (uint32_t row = 0; row < rows; row++)
        std::memcpy(dst + row * row_bytes, src + row * row_pitch, row_bytes);
    device->device->unmapBuffer(staging);
    return DGPU_OK;
}

extern "C" int dgpu_texture_create(DgpuDevice* device, const DgpuTextureDesc* d, DgpuObject** out)
{
    DeviceScope deviceScope(device->device);
    *out = nullptr;
    Format format = textureFormat(d->format);
    if (format == Format::Undefined)
        return fail(DGPU_INVALID, "unknown texture format");
    TextureDesc desc;
    desc.type = d->layers > 1 ? TextureType::Texture2DArray : TextureType::Texture2D;
    if (d->samples > 1)
        desc.type = d->layers > 1 ? TextureType::Texture2DMSArray : TextureType::Texture2DMS;
    desc.size = {d->width, d->height, 1};
    desc.arrayLength = d->layers;
    desc.mipCount = d->mips;
    desc.sampleCount = d->samples;
    desc.format = format;
    desc.usage = TextureUsage::CopySource | TextureUsage::CopyDestination;
    if (d->samples <= 1)
        desc.usage |= TextureUsage::ShaderResource;
    if (d->usage & 1)
        desc.usage |= isDepth(format) ? TextureUsage::DepthStencil : TextureUsage::RenderTarget;
    if (d->usage & 2)
        desc.usage |= TextureUsage::UnorderedAccess;
    desc.defaultState = (d->usage & 1) ? (isDepth(format) ? ResourceState::DepthWrite : ResourceState::RenderTarget)
                                       : ResourceState::ShaderResource;
    ComPtr<ITexture> texture;
    Result result = device->device->createTexture(desc, nullptr, texture.writeRef());
    if (SLANG_FAILED(result) || !texture)
        return failResult(device, result, "texture creation failed");
    *out = texture.detach();
    return DGPU_OK;
}

extern "C" int dgpu_texture_readback_layout(DgpuObject* texture, uint32_t mip, DgpuReadbackLayout* out)
{
    ITexture* t = static_cast<ITexture*>(texture);
    SubresourceLayout layout;
    if (SLANG_FAILED(t->getSubresourceLayout(mip, &layout)))
        return fail(DGPU_INVALID, "the texture has no such mip level");
    SubresourceLayout tight;
    if (SLANG_FAILED(t->getSubresourceLayout(mip, 1, &tight)))
        return fail(DGPU_INVALID, "the texture has no such mip level");
    out->row_pitch = layout.rowPitch;
    out->row_bytes = tight.rowPitch;
    out->rows = uint32_t(layout.size.height * layout.size.depth);
    out->staging_size = layout.rowPitch * out->rows;
    return DGPU_OK;
}

extern "C" int dgpu_sampler_create(DgpuDevice* device, const int32_t* d, DgpuObject** out)
{
    DeviceScope deviceScope(device->device);
    *out = nullptr;
    SamplerDesc desc;
    desc.minFilter = filter(d[0]);
    desc.magFilter = filter(d[1]);
    desc.mipFilter = filter(d[2]);
    desc.addressU = address(d[3]);
    desc.addressV = address(d[4]);
    desc.addressW = address(d[5]);
    if (d[6] > 0)
    {
        desc.reductionOp = TextureReductionOp::Comparison;
        desc.comparisonFunc = compare(uint32_t(d[6] - 1));
    }
    desc.maxAnisotropy = d[7] > 1 ? uint32_t(d[7]) : 1;
    ComPtr<ISampler> sampler;
    Result result = device->device->createSampler(desc, sampler.writeRef());
    if (SLANG_FAILED(result) || !sampler)
        return failResult(device, result, "sampler creation failed");
    *out = sampler.detach();
    return DGPU_OK;
}

extern "C" int dgpu_descriptor_handle(DgpuDevice* device, DgpuObject* resource, int kind, uint64_t* out)
{
    DeviceScope deviceScope(device->device);
    *out = 0;
    DescriptorHandle handle;
    Result result = SLANG_E_INVALID_ARG;
    switch (kind)
    {
    case 1:
        result = static_cast<IBuffer*>(resource)->getDescriptorHandle(DescriptorHandleAccess::ReadWrite, Format::Undefined, kEntireBuffer, &handle);
        break;
    case 2:
    case 4:
    {
        ComPtr<ITextureView> view;
        result = static_cast<ITexture*>(resource)->getDefaultView(view.writeRef());
        if (SLANG_SUCCEEDED(result))
            result = view->getDescriptorHandle(kind == 2 ? DescriptorHandleAccess::Read : DescriptorHandleAccess::ReadWrite, &handle);
        break;
    }
    case 3:
        result = static_cast<ISampler*>(resource)->getDescriptorHandle(&handle);
        break;
    default:
        break;
    }
    if (SLANG_FAILED(result) || !handle)
        return failResult(device, result, "the device gave no bindless handle for the resource");
    *out = handle.value;
    return DGPU_OK;
}

namespace {

CooperativeVectorComponentType mlpComponent(int component)
{
    return component == 0 ? CooperativeVectorComponentType::Float16 : CooperativeVectorComponentType::Float32;
}

CooperativeVectorMatrixDesc mlpMatrix(
    uint32_t rows,
    uint32_t columns,
    int component,
    CooperativeVectorMatrixLayout layout,
    size_t size
)
{
    CooperativeVectorMatrixDesc desc = {};
    desc.rowCount = rows;
    desc.colCount = columns;
    desc.componentType = mlpComponent(component);
    desc.layout = layout;
    desc.size = size;
    desc.offset = 0;
    desc.rowColumnStride = layout == CooperativeVectorMatrixLayout::RowMajor ? size / rows : 0;
    return desc;
}

} // namespace

extern "C" int dgpu_mlp_optimal_size(DgpuDevice* device, uint32_t rows, uint32_t columns, int component, uint64_t* out)
{
    DeviceScope deviceScope(device->device);
    *out = 0;
    size_t size = 0;
    Result result = device->device->getCooperativeVectorMatrixSize(
        rows,
        columns,
        mlpComponent(component),
        CooperativeVectorMatrixLayout::InferencingOptimal,
        0,
        &size
    );
    if (SLANG_FAILED(result) || size == 0)
        return failResult(device, result, "the device reported no cooperative-vector matrix size");
    *out = size;
    return DGPU_OK;
}

extern "C" int dgpu_mlp_convert_optimal(
    DgpuDevice* device,
    const void* src,
    uint32_t rows,
    uint32_t columns,
    int component,
    void* dst,
    uint64_t dst_size
)
{
    DeviceScope deviceScope(device->device);
    size_t element = component == 0 ? 2 : 4;
    size_t src_size = size_t(rows) * columns * element;
    CooperativeVectorMatrixDesc from =
        mlpMatrix(rows, columns, component, CooperativeVectorMatrixLayout::RowMajor, src_size);
    CooperativeVectorMatrixDesc to =
        mlpMatrix(rows, columns, component, CooperativeVectorMatrixLayout::InferencingOptimal, size_t(dst_size));
    Result result = device->device->convertCooperativeVectorMatrix(dst, size_t(dst_size), &to, src, src_size, &from, 1);
    if (SLANG_FAILED(result))
        return failResult(device, result, "converting the MLP weights to the device layout failed");
    return DGPU_OK;
}
