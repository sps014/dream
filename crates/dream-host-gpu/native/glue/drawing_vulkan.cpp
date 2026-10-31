#include "glue/internal.h"

#if defined(_WIN32)
#define VK_USE_PLATFORM_WIN32_KHR
#define VK_NO_PROTOTYPES
#include <windows.h>
#include <vulkan/vulkan.h>
#include <cstring>

using namespace rhi;
using namespace dream::gpu;

namespace {
int vkFailure(DgpuDevice* device, VkResult result, const char* what)
{
    auto detail = std::string(what) + " (VkResult " + std::to_string(result) + ")";
    if (result == VK_ERROR_DEVICE_LOST)
    {
        device->sink.handleMessage(DebugMessageType::Error, DebugMessageSource::Driver, "Vulkan device lost during drawing transfer");
        return fail(DGPU_DEVICE_LOST, detail);
    }
    return fail(result == VK_ERROR_OUT_OF_HOST_MEMORY || result == VK_ERROR_OUT_OF_DEVICE_MEMORY ?
        DGPU_OUT_OF_MEMORY : DGPU_FAILED, detail);
}
struct Image
{
    std::atomic<unsigned> refs{1};
    VkDevice device = VK_NULL_HANDLE;
    ComPtr<IFence> fence;
    VkImage image = VK_NULL_HANDLE;
    VkDeviceMemory memory = VK_NULL_HANDLE;
    HANDLE handle = nullptr, semaphoreHandle = nullptr;
    VkDeviceSize size = 0;
    uint32_t memoryType = 0, family = 0;
    uint8_t deviceUuid[VK_UUID_SIZE]{}, driverUuid[VK_UUID_SIZE]{};
    PFN_vkDestroyImage destroyImage = nullptr;
    PFN_vkFreeMemory freeMemory = nullptr;
    PFN_vkCmdPipelineBarrier barrier = nullptr;
    PFN_vkGetSemaphoreWin32HandleKHR exportSemaphore = nullptr;
    ~Image()
    {
        if (image) destroyImage(device, image, nullptr);
        if (memory) freeMemory(device, memory, nullptr);
        if (handle) CloseHandle(handle);
        if (semaphoreHandle) CloseHandle(semaphoreHandle);
    }
};
void SLANG_MCALL retain(void* object) { ++static_cast<Image*>(object)->refs; }
void SLANG_MCALL release(void* object)
{
    auto image = static_cast<Image*>(object);
    if (--image->refs == 0) delete image;
}
// Each callback retains the allocation until its command buffer is retired by the queue.
void SLANG_MCALL ownership(const ExecuteCallbackContext* context, void* object, const void* data, Size)
{
    auto image = static_cast<Image*>(object);
    auto mode = *static_cast<const uint32_t*>(data);
    auto command = reinterpret_cast<VkCommandBuffer>(context->nativeHandle.value);
    VkImageMemoryBarrier barrier{VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER};
    barrier.image = image->image;
    barrier.subresourceRange = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
    barrier.oldLayout = mode == 0 ? VK_IMAGE_LAYOUT_UNDEFINED : VK_IMAGE_LAYOUT_GENERAL;
    barrier.newLayout = VK_IMAGE_LAYOUT_GENERAL;
    barrier.srcQueueFamilyIndex = mode == 0 ? VK_QUEUE_FAMILY_IGNORED : mode == 1 ? VK_QUEUE_FAMILY_EXTERNAL : image->family;
    barrier.dstQueueFamilyIndex = mode == 0 ? VK_QUEUE_FAMILY_IGNORED : mode == 1 ? image->family : VK_QUEUE_FAMILY_EXTERNAL;
    barrier.srcAccessMask = mode == 2 ? VK_ACCESS_MEMORY_READ_BIT | VK_ACCESS_MEMORY_WRITE_BIT : 0;
    barrier.dstAccessMask = mode == 2 ? 0 : VK_ACCESS_MEMORY_READ_BIT | VK_ACCESS_MEMORY_WRITE_BIT;
    image->barrier(command, VK_PIPELINE_STAGE_ALL_COMMANDS_BIT, VK_PIPELINE_STAGE_ALL_COMMANDS_BIT,
        0, 0, nullptr, 0, nullptr, 1, &barrier);
}
void transfer(ICommandEncoder* encoder, Image* image, uint32_t mode)
{
    ExecuteCallbackDesc callback;
    callback.callback = ownership;
    callback.userObject = image;
    callback.retainUserObject = retain;
    callback.releaseUserObject = release;
    callback.userData = &mode;
    callback.userDataSize = sizeof(mode);
    encoder->executeCallback(callback);
}

int create(DgpuDevice* device, const TextureDesc& desc, std::shared_ptr<void>& allocation, ComPtr<ITexture>& texture)
{
    auto loader = GetModuleHandleA("vulkan-1.dll");
    auto getInstance = loader ? reinterpret_cast<PFN_vkGetInstanceProcAddr>(GetProcAddress(loader, "vkGetInstanceProcAddr")) : nullptr;
    if (!getInstance) return DGPU_UNSUPPORTED;
    DeviceNativeHandles native;
    if (SLANG_FAILED(device->device->getNativeDeviceHandles(&native))) return DGPU_UNSUPPORTED;
    auto instance = reinterpret_cast<VkInstance>(native.handles[0].value);
    auto physical = reinterpret_cast<VkPhysicalDevice>(native.handles[1].value);
    auto logical = reinterpret_cast<VkDevice>(native.handles[2].value);
    auto getDevice = reinterpret_cast<PFN_vkGetDeviceProcAddr>(getInstance(instance, "vkGetDeviceProcAddr"));
#define INSTANCE(name) auto name = reinterpret_cast<PFN_##name>(getInstance(instance, #name))
#define DEVICE(name) auto name = reinterpret_cast<PFN_##name>(getDevice(logical, #name))
    INSTANCE(vkGetPhysicalDeviceImageFormatProperties2);
    INSTANCE(vkGetPhysicalDeviceMemoryProperties);
    INSTANCE(vkGetPhysicalDeviceExternalSemaphoreProperties);
    INSTANCE(vkGetPhysicalDeviceProperties2);
    INSTANCE(vkGetPhysicalDeviceQueueFamilyProperties);
    DEVICE(vkCreateImage); DEVICE(vkGetImageMemoryRequirements); DEVICE(vkAllocateMemory);
    DEVICE(vkBindImageMemory); DEVICE(vkGetMemoryWin32HandleKHR);
    DEVICE(vkGetSemaphoreWin32HandleKHR);
    if (!vkGetMemoryWin32HandleKHR || !vkGetSemaphoreWin32HandleKHR) return DGPU_UNSUPPORTED;
    VkSemaphoreTypeCreateInfo timeline{VK_STRUCTURE_TYPE_SEMAPHORE_TYPE_CREATE_INFO};
    timeline.semaphoreType = VK_SEMAPHORE_TYPE_TIMELINE;
    VkPhysicalDeviceExternalSemaphoreInfo semaphoreInfo{VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_EXTERNAL_SEMAPHORE_INFO};
    semaphoreInfo.pNext = &timeline;
    semaphoreInfo.handleType = VK_EXTERNAL_SEMAPHORE_HANDLE_TYPE_OPAQUE_WIN32_BIT;
    VkExternalSemaphoreProperties semaphoreProperties{VK_STRUCTURE_TYPE_EXTERNAL_SEMAPHORE_PROPERTIES};
    vkGetPhysicalDeviceExternalSemaphoreProperties(physical, &semaphoreInfo, &semaphoreProperties);
    if ((semaphoreProperties.externalSemaphoreFeatures &
        (VK_EXTERNAL_SEMAPHORE_FEATURE_EXPORTABLE_BIT | VK_EXTERNAL_SEMAPHORE_FEATURE_IMPORTABLE_BIT)) !=
        (VK_EXTERNAL_SEMAPHORE_FEATURE_EXPORTABLE_BIT | VK_EXTERNAL_SEMAPHORE_FEATURE_IMPORTABLE_BIT)) return DGPU_UNSUPPORTED;
    auto usage = VK_IMAGE_USAGE_TRANSFER_SRC_BIT | VK_IMAGE_USAGE_TRANSFER_DST_BIT;
    if (desc.format == Format::BGRA8Unorm) usage |= VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT;
    auto format = desc.format == Format::BGRA8Unorm ? VK_FORMAT_B8G8R8A8_UNORM : VK_FORMAT_R8G8B8A8_UNORM;
    VkPhysicalDeviceExternalImageFormatInfo external{VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_EXTERNAL_IMAGE_FORMAT_INFO};
    external.handleType = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_WIN32_BIT;
    VkPhysicalDeviceImageFormatInfo2 query{VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_IMAGE_FORMAT_INFO_2};
    query.pNext = &external; query.format = format; query.type = VK_IMAGE_TYPE_2D;
    query.tiling = VK_IMAGE_TILING_OPTIMAL; query.usage = usage;
    VkExternalImageFormatProperties shared{VK_STRUCTURE_TYPE_EXTERNAL_IMAGE_FORMAT_PROPERTIES};
    VkImageFormatProperties2 properties{VK_STRUCTURE_TYPE_IMAGE_FORMAT_PROPERTIES_2};
    properties.pNext = &shared;
    auto queried = vkGetPhysicalDeviceImageFormatProperties2(physical, &query, &properties);
    if (queried != VK_SUCCESS && queried != VK_ERROR_FORMAT_NOT_SUPPORTED)
        return vkFailure(device, queried, "querying Vulkan drawing image support failed");
    if (queried == VK_ERROR_FORMAT_NOT_SUPPORTED ||
        (shared.externalMemoryProperties.externalMemoryFeatures &
            (VK_EXTERNAL_MEMORY_FEATURE_EXPORTABLE_BIT | VK_EXTERNAL_MEMORY_FEATURE_IMPORTABLE_BIT)) !=
            (VK_EXTERNAL_MEMORY_FEATURE_EXPORTABLE_BIT | VK_EXTERNAL_MEMORY_FEATURE_IMPORTABLE_BIT))
        return DGPU_UNSUPPORTED;
    auto image = new Image;
    allocation = std::shared_ptr<void>(image, release);
    image->device = logical;
    image->exportSemaphore = vkGetSemaphoreWin32HandleKHR;
    image->destroyImage = reinterpret_cast<PFN_vkDestroyImage>(getDevice(logical, "vkDestroyImage"));
    image->freeMemory = reinterpret_cast<PFN_vkFreeMemory>(getDevice(logical, "vkFreeMemory"));
    image->barrier = reinterpret_cast<PFN_vkCmdPipelineBarrier>(getDevice(logical, "vkCmdPipelineBarrier"));
    VkPhysicalDeviceIDProperties ids{VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_ID_PROPERTIES};
    VkPhysicalDeviceProperties2 physicalProperties{VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2};
    physicalProperties.pNext = &ids;
    vkGetPhysicalDeviceProperties2(physical, &physicalProperties);
    memcpy(image->deviceUuid, ids.deviceUUID, VK_UUID_SIZE);
    memcpy(image->driverUuid, ids.driverUUID, VK_UUID_SIZE);
    uint32_t count = 0;
    vkGetPhysicalDeviceQueueFamilyProperties(physical, &count, nullptr);
    std::vector<VkQueueFamilyProperties> families(count);
    vkGetPhysicalDeviceQueueFamilyProperties(physical, &count, families.data());
    image->family = UINT32_MAX;
    for (uint32_t i = 0; i < count; ++i)
        if ((families[i].queueFlags & (VK_QUEUE_GRAPHICS_BIT | VK_QUEUE_COMPUTE_BIT)) ==
            (VK_QUEUE_GRAPHICS_BIT | VK_QUEUE_COMPUTE_BIT)) { image->family = i; break; }
    if (image->family == UINT32_MAX) return DGPU_UNSUPPORTED;
    VkExternalMemoryImageCreateInfo externalImage{VK_STRUCTURE_TYPE_EXTERNAL_MEMORY_IMAGE_CREATE_INFO};
    externalImage.handleTypes = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_WIN32_BIT;
    VkImageCreateInfo info{VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO};
    info.pNext = &externalImage; info.imageType = VK_IMAGE_TYPE_2D; info.format = format;
    info.extent = {desc.size.width, desc.size.height, 1}; info.mipLevels = 1; info.arrayLayers = 1;
    info.samples = VK_SAMPLE_COUNT_1_BIT; info.tiling = VK_IMAGE_TILING_OPTIMAL;
    info.usage = usage; info.sharingMode = VK_SHARING_MODE_EXCLUSIVE;
    auto created = vkCreateImage(logical, &info, nullptr, &image->image);
    if (created != VK_SUCCESS) return vkFailure(device, created, "creating the Vulkan drawing image failed");
    VkMemoryRequirements requirements;
    vkGetImageMemoryRequirements(logical, image->image, &requirements);
    VkPhysicalDeviceMemoryProperties memory;
    vkGetPhysicalDeviceMemoryProperties(physical, &memory);
    image->memoryType = UINT32_MAX;
    for (uint32_t i = 0; i < memory.memoryTypeCount; ++i)
        if ((requirements.memoryTypeBits & (1u << i)) && (memory.memoryTypes[i].propertyFlags & VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT))
        { image->memoryType = i; break; }
    if (image->memoryType == UINT32_MAX) return DGPU_UNSUPPORTED;
    image->size = requirements.size;
    if (image->size > 256ull * 1024 * 1024) return fail(DGPU_OUT_OF_MEMORY, "Vulkan drawing allocation exceeds 256 MiB");
    VkMemoryDedicatedAllocateInfo dedicated{VK_STRUCTURE_TYPE_MEMORY_DEDICATED_ALLOCATE_INFO};
    dedicated.image = image->image;
    VkExportMemoryAllocateInfo exportInfo{VK_STRUCTURE_TYPE_EXPORT_MEMORY_ALLOCATE_INFO};
    exportInfo.pNext = &dedicated; exportInfo.handleTypes = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_WIN32_BIT;
    VkMemoryAllocateInfo alloc{VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
    alloc.pNext = &exportInfo; alloc.allocationSize = image->size; alloc.memoryTypeIndex = image->memoryType;
    auto allocated = vkAllocateMemory(logical, &alloc, nullptr, &image->memory);
    if (allocated != VK_SUCCESS) return vkFailure(device, allocated, "allocating the Vulkan drawing image failed");
    auto bound = vkBindImageMemory(logical, image->image, image->memory, 0);
    if (bound != VK_SUCCESS) return vkFailure(device, bound, "binding the Vulkan drawing allocation failed");
    VkMemoryGetWin32HandleInfoKHR handle{VK_STRUCTURE_TYPE_MEMORY_GET_WIN32_HANDLE_INFO_KHR};
    handle.memory = image->memory; handle.handleType = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_WIN32_BIT;
    auto exported = vkGetMemoryWin32HandleKHR(logical, &handle, &image->handle);
    if (exported != VK_SUCCESS) return vkFailure(device, exported, "exporting the Vulkan drawing allocation failed");
    NativeHandle wrapped;
    wrapped.type = NativeHandleType::VkImage;
    wrapped.value = uint64_t(image->image);
    auto result = device->device->createTextureFromNativeHandle(wrapped, desc, texture.writeRef());
    if (SLANG_FAILED(result)) return failResult(device, result, "wrapping the Vulkan drawing image failed");
    return DGPU_OK;
}
}

int drawingVulkanCopy(DgpuDevice* device, DgpuObject* object, DgpuDrawingProducer producer,
    void* context, char* error, size_t capacity, uint64_t* submitted)
{
    DeviceScope scope(device->device);
    auto target = static_cast<ITexture*>(object);
    const auto& desc = target->getDesc();
    auto format = dreamTextureFormat(desc.format);
    if (format > 3 || desc.sampleCount != 1 || (desc.type != TextureType::Texture2D && desc.type != TextureType::Texture2DArray) ||
        desc.arrayLength < 1 || desc.size.width > 8192 || desc.size.height > 8192 || uint64_t(desc.size.width) * desc.size.height > 16777216)
        return fail(DGPU_INVALID, "drawing copies need a matching single-sample RGBA8/BGRA8 2D texture");
    auto sharedFormat = format >= 2 ? Format::BGRA8Unorm : Format::RGBA8Unorm;
    if (!device->drawingTexture || device->drawingTexture->getDesc().format != sharedFormat ||
        device->drawingTexture->getDesc().size.width != desc.size.width || device->drawingTexture->getDesc().size.height != desc.size.height)
    {
        uint64_t completed = 0;
        if (device->drawingFence)
        {
            auto result = device->drawingFence->getCurrentValue(&completed);
            if (SLANG_FAILED(result)) return failResult(device, result, "reading the Vulkan drawing fence failed");
            if (completed < device->drawingValue) return fail(DGPU_OUT_OF_MEMORY, "the Vulkan drawing target is busy");
        }
        TextureDesc shared;
        shared.size = {desc.size.width, desc.size.height, 1}; shared.format = sharedFormat;
        shared.usage = TextureUsage::CopySource | TextureUsage::CopyDestination;
        if (format >= 2) shared.usage |= TextureUsage::RenderTarget;
        shared.defaultState = ResourceState::General;
        std::shared_ptr<void> allocation;
        ComPtr<ITexture> texture;
        auto status = create(device, shared, allocation, texture);
        if (status != DGPU_OK) return status;
        FenceDesc fenceDesc; fenceDesc.isShared = true;
        ComPtr<IFence> fence;
        auto result = device->device->createFence(fenceDesc, fence.writeRef());
        if (SLANG_FAILED(result)) return failResult(device, result, "creating the Vulkan drawing semaphore failed");
        auto image = static_cast<Image*>(allocation.get());
        // A failed later handoff can invalidate the cache while older submissions still use this semaphore.
        image->fence = fence;
        NativeHandle semaphore;
        result = fence->getNativeHandle(&semaphore);
        if (SLANG_FAILED(result)) return failResult(device, result, "reading the Vulkan drawing semaphore failed");
        VkSemaphoreGetWin32HandleInfoKHR semaphoreInfo{VK_STRUCTURE_TYPE_SEMAPHORE_GET_WIN32_HANDLE_INFO_KHR};
        semaphoreInfo.semaphore = VkSemaphore(semaphore.value);
        semaphoreInfo.handleType = VK_EXTERNAL_SEMAPHORE_HANDLE_TYPE_OPAQUE_WIN32_BIT;
        auto exported = image->exportSemaphore(image->device, &semaphoreInfo, &image->semaphoreHandle);
        if (exported != VK_SUCCESS) return vkFailure(device, exported, "exporting the Vulkan drawing semaphore failed");
        ComPtr<ICommandEncoder> encoder;
        result = device->queue->createCommandEncoder(encoder.writeRef());
        if (SLANG_FAILED(result)) return failResult(device, result, "creating the Vulkan initialization encoder failed");
        transfer(encoder, static_cast<Image*>(allocation.get()), 0);
        transfer(encoder, static_cast<Image*>(allocation.get()), 2);
        ComPtr<ICommandBuffer> commands;
        result = encoder->finish(commands.writeRef());
        if (SLANG_FAILED(result)) return failResult(device, result, "finishing Vulkan initialization failed");
        ICommandBuffer* buffers[] = {commands}; IFence* fences[] = {fence}; uint64_t value = 1;
        SubmitDesc init;
        init.commandBuffers = buffers; init.commandBufferCount = 1;
        init.signalFences = fences; init.signalFenceValues = &value; init.signalFenceCount = 1;
        result = device->queue->submit(init);
        if (SLANG_FAILED(result)) return failResult(device, result, "submitting Vulkan initialization failed");
        result = device->queue->waitOnHost();
        if (SLANG_FAILED(result)) return failResult(device, result, "waiting for Vulkan initialization failed");
        auto generation = nextDrawingId();
        if (!generation) return fail(DGPU_FAILED, "drawing generations exhausted");
        device->drawingTexture = texture; device->drawingFence = fence; device->drawingExternal = allocation;
        device->drawingId = generation; device->drawingValue = 1; ++device->drawingAllocations;
    }
    if (device->drawingValue > UINT64_MAX - 2) return fail(DGPU_FAILED, "Vulkan drawing semaphore values exhausted");
    auto image = static_cast<Image*>(device->drawingExternal.get());
    DgpuDrawingTarget info{};
    info.id = device->drawingId; info.texture = uintptr_t(image->handle); info.fence = uintptr_t(image->semaphoreHandle);
    info.wait_value = device->drawingValue; info.signal_value = device->drawingValue + 1;
    info.width = desc.size.width; info.height = desc.size.height; info.format = format; info.backend = 3;
    memcpy(info.device_uuid, image->deviceUuid, 16); memcpy(info.driver_uuid, image->driverUuid, 16);
    info.memory_size = image->size; info.memory_type = image->memoryType;
    auto poison = [&]() { device->drawingTexture = nullptr; device->drawingFence = nullptr; device->drawingExternal.reset(); device->drawingValue = 0; };
    auto produced = producer(context, &info, error, capacity);
    if (produced == 0) return DGPU_UNSUPPORTED;
    if (produced < 0) { poison(); return fail(DGPU_FAILED, "Vulkan drawing producer failed"); }
    ComPtr<ICommandEncoder> encoder;
    auto result = device->queue->createCommandEncoder(encoder.writeRef());
    if (SLANG_FAILED(result)) { poison(); return failResult(device, result, "creating the Vulkan copy encoder failed"); }
    transfer(encoder, image, 1);
    SubresourceRange first{0, 1, 0, 1};
    encoder->copyTexture(target, first, {0, 0, 0}, device->drawingTexture, first, {0, 0, 0}, {desc.size.width, desc.size.height, 1});
    encoder->setTextureState(device->drawingTexture, ResourceState::General);
    encoder->setTextureState(target, desc.defaultState);
    transfer(encoder, image, 2);
    ComPtr<ICommandBuffer> commands;
    result = encoder->finish(commands.writeRef());
    if (SLANG_FAILED(result)) { poison(); return failResult(device, result, "finishing the Vulkan drawing copy failed"); }
    auto ready = info.signal_value, consumed = ready + 1, fenceValue = ++device->nextFenceValue;
    ICommandBuffer* buffers[] = {commands}; IFence* waits[] = {device->drawingFence};
    IFence* signals[] = {device->drawingFence, device->fence}; uint64_t values[] = {consumed, fenceValue};
    SubmitDesc submit;
    submit.commandBuffers = buffers; submit.commandBufferCount = 1;
    submit.waitFences = waits; submit.waitFenceValues = &ready; submit.waitFenceCount = 1;
    submit.signalFences = signals; submit.signalFenceValues = values; submit.signalFenceCount = 2;
    result = device->queue->submit(submit);
    if (SLANG_FAILED(result)) { poison(); return failResult(device, result, "submitting the Vulkan drawing copy failed"); }
    device->drawingValue = consumed; *submitted = fenceValue;
    return DGPU_OK;
}
#endif
