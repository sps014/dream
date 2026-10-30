#include "glue/internal.h"

#if defined(_WIN32)
#include <d3d12.h>
#endif

using namespace rhi;
using namespace dream::gpu;

extern "C" uint64_t dgpu_drawing_allocations(DgpuDevice* device)
{
    return device->drawingAllocations;
}

extern "C" int dgpu_drawing_copy(
    DgpuDevice* device, DgpuObject* object, DgpuDrawingProducer producer,
    void* context, char* error, size_t capacity, uint64_t* submitted)
{
#if !defined(_WIN32)
    (void)device; (void)object; (void)producer; (void)context;
    (void)error; (void)capacity; (void)submitted;
    return DGPU_UNSUPPORTED;
#else
    if (device->backend != 1)
        return DGPU_UNSUPPORTED;
    DeviceScope scope(device->device);
    auto target = static_cast<ITexture*>(object);
    const auto& desc = target->getDesc();
    auto format = dreamTextureFormat(desc.format);
    if (format > 3 || desc.sampleCount != 1 || (desc.type != TextureType::Texture2D && desc.type != TextureType::Texture2DArray) ||
        desc.arrayLength < 1 || desc.size.width > 8192 || desc.size.height > 8192 ||
        uint64_t(desc.size.width) * desc.size.height > 16777216)
        return fail(DGPU_INVALID, "drawing copies need a matching single-sample RGBA8/BGRA8 2D texture");
    DeviceNativeHandles native;
    auto nativeResult = device->device->getNativeDeviceHandles(&native);
    if (SLANG_FAILED(nativeResult))
        return failResult(device, nativeResult, "reading the native device failed");
    if (native.handles[0].type != NativeHandleType::D3D12Device)
        return fail(DGPU_FAILED, "the D3D12 device has no native handle");
    auto raw = reinterpret_cast<ID3D12Device*>(native.handles[0].value);
    auto luid = raw->GetAdapterLuid();
    auto sharedFormat = format >= 2 ? Format::BGRA8Unorm : Format::RGBA8Unorm;
    bool replace = !device->drawingTexture || device->drawingTexture->getDesc().format != sharedFormat ||
        device->drawingTexture->getDesc().size.width != desc.size.width ||
        device->drawingTexture->getDesc().size.height != desc.size.height;
    if (replace)
    {
        uint64_t completed = 0;
        if (device->drawingFence)
        {
            auto status = device->drawingFence->getCurrentValue(&completed);
            if (SLANG_FAILED(status))
                return failResult(device, status, "reading the drawing fence failed");
            if (completed < device->drawingValue)
                return fail(DGPU_OUT_OF_MEMORY, "the shared drawing target is busy; wait for earlier GPU work before changing its size or format");
        }
        TextureDesc shared;
        shared.size = desc.size;
        shared.format = sharedFormat;
        shared.usage = TextureUsage::Shared | TextureUsage::CopySource | TextureUsage::CopyDestination;
        if (format >= 2) shared.usage |= TextureUsage::RenderTarget;
        shared.defaultState = ResourceState::General;
        ComPtr<ITexture> texture;
        auto result = device->device->createTexture(shared, nullptr, texture.writeRef());
        if (SLANG_FAILED(result))
            return failResult(device, result, "creating a shared drawing texture failed");
        FenceDesc fenceDesc;
        fenceDesc.isShared = true;
        ComPtr<IFence> fence;
        result = device->device->createFence(fenceDesc, fence.writeRef());
        if (SLANG_FAILED(result))
            return failResult(device, result, "creating a shared drawing fence failed");
        static std::atomic<uint64_t> ids{0};
        device->drawingTexture = texture;
        device->drawingFence = fence;
        device->drawingId = ++ids;
        device->drawingValue = 0;
        ++device->drawingAllocations;
    }
    if (device->drawingValue > UINT64_MAX - 2)
        return fail(DGPU_FAILED, "drawing fence values exhausted");
    NativeHandle textureHandle, fenceHandle;
    auto result = device->drawingTexture->getSharedHandle(&textureHandle);
    if (SLANG_FAILED(result))
        return failResult(device, result, "exporting the drawing texture failed");
    result = device->drawingFence->getSharedHandle(&fenceHandle);
    if (SLANG_FAILED(result))
        return failResult(device, result, "exporting the drawing fence failed");
    DgpuDrawingTarget transfer = {
        device->drawingId, (uint64_t(uint32_t(luid.HighPart)) << 32) | luid.LowPart,
        uintptr_t(textureHandle.value), uintptr_t(fenceHandle.value),
        device->drawingValue, device->drawingValue + 1,
        desc.size.width, desc.size.height, format,
    };
    // Do not reuse a failed handoff: an old ready signal could race the next producer.
    auto poison = [&]() {
        device->drawingTexture = nullptr;
        device->drawingFence = nullptr;
        device->drawingValue = 0;
    };
    int produced = producer(context, &transfer, error, capacity);
    if (produced == 0) return DGPU_UNSUPPORTED;
    if (produced < 0)
    {
        poison();
        return fail(DGPU_FAILED, "drawing producer failed");
    }
    ComPtr<ICommandEncoder> encoder;
    result = device->queue->createCommandEncoder(encoder.writeRef());
    if (SLANG_FAILED(result))
    {
        poison();
        return failResult(device, result, "drawing copy encoder failed");
    }
    SubresourceRange first = {0, 1, 0, 1};
    encoder->copyTexture(target, first, {0, 0, 0}, device->drawingTexture, first, {0, 0, 0}, desc.size);
    encoder->setTextureState(device->drawingTexture, ResourceState::General);
    encoder->setTextureState(target, desc.defaultState);
    ComPtr<ICommandBuffer> commands;
    result = encoder->finish(commands.writeRef());
    if (SLANG_FAILED(result))
    {
        poison();
        return failResult(device, result, "finishing the drawing copy failed");
    }
    uint64_t ready = transfer.signal_value;
    uint64_t consumed = ready + 1;
    uint64_t fenceValue = ++device->nextFenceValue;
    ICommandBuffer* buffers[] = {commands};
    IFence* waits[] = {device->drawingFence};
    IFence* signals[] = {device->drawingFence, device->fence};
    uint64_t values[] = {consumed, fenceValue};
    SubmitDesc submit;
    submit.commandBuffers = buffers; submit.commandBufferCount = 1;
    submit.waitFences = waits; submit.waitFenceValues = &ready; submit.waitFenceCount = 1;
    submit.signalFences = signals; submit.signalFenceValues = values; submit.signalFenceCount = 2;
    result = device->queue->submit(submit);
    if (SLANG_FAILED(result))
    {
        poison();
        return failResult(device, result, "submitting the drawing copy failed");
    }
    device->drawingValue = consumed;
    *submitted = fenceValue;
    return DGPU_OK;
#endif
}
