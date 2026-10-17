#include "glue/internal.h"

#include "loader_check.h"
#include "replay/replay.h"

#include <algorithm>
#include <cctype>

using namespace rhi;

namespace dream::gpu {

namespace {

thread_local std::string t_message;

bool mentionsLoss(const char* message)
{
    std::string lower(message);
    std::transform(lower.begin(), lower.end(), lower.begin(), [](unsigned char c) { return char(std::tolower(c)); });
    return lower.find("device lost") != std::string::npos || lower.find("device_lost") != std::string::npos ||
           lower.find("device removed") != std::string::npos || lower.find("device_removed") != std::string::npos;
}

} // namespace

int fail(int status, std::string message)
{
    t_message = std::move(message);
    return status;
}

int failResult(DgpuDevice* device, Result result, const char* what)
{
    std::string reason;
    if (device && device->sink.lost(&reason))
        return fail(DGPU_DEVICE_LOST, reason);
    std::string detail = device ? device->sink.takeLastError() : std::string();
    std::string message = std::string(what) + (detail.empty() ? "" : ": " + detail);
    if (result == SLANG_E_OUT_OF_MEMORY)
        return fail(DGPU_OUT_OF_MEMORY, message);
    if (result == SLANG_E_NOT_AVAILABLE || result == SLANG_E_NOT_IMPLEMENTED)
        return fail(DGPU_UNSUPPORTED, message);
    if (result == SLANG_E_INVALID_ARG)
        return fail(DGPU_INVALID, message);
    return fail(DGPU_FAILED, message);
}

void DebugSink::handleMessage(DebugMessageType type, DebugMessageSource, const char* message)
{
    if (type != DebugMessageType::Error || !message)
        return;
    std::lock_guard<std::mutex> lock(m_mutex);
    m_lastError = message;
    if (mentionsLoss(message) && !m_lost.exchange(true))
        m_lostReason = message;
}

std::string DebugSink::takeLastError()
{
    std::lock_guard<std::mutex> lock(m_mutex);
    return std::exchange(m_lastError, std::string());
}

bool DebugSink::lost(std::string* reason)
{
    if (!m_lost.load())
        return false;
    std::lock_guard<std::mutex> lock(m_mutex);
    if (reason)
        *reason = m_lostReason;
    return true;
}

DeviceType deviceType(int backend)
{
    switch (backend)
    {
    case 1:
        return DeviceType::D3D12;
    case 2:
        return DeviceType::D3D11;
    case 3:
        return DeviceType::Vulkan;
    case 4:
        return DeviceType::Metal;
    case 5:
        return DeviceType::CUDA;
    case 6:
        return DeviceType::WGPU;
    default:
        return DeviceType::Default;
    }
}

} // namespace dream::gpu

using namespace dream::gpu;

extern "C" const char* dgpu_message(void)
{
    return t_message.c_str();
}

extern "C" int dgpu_backend_compiled(int backend)
{
    DeviceType type = deviceType(backend);
    return type != DeviceType::Default && getRHI()->isDeviceTypeSupported(type) ? 1 : 0;
}

extern "C" int dgpu_device_create(int backend, DgpuDevice** out)
{
    *out = nullptr;
    watchLoadedImages();
    DeviceType type = deviceType(backend);
    if (type == DeviceType::Default || !getRHI()->isDeviceTypeSupported(type))
        return fail(DGPU_UNSUPPORTED, "this host was built without the backend");
    auto device = new DgpuDevice();
    device->backend = backend;
    DeviceDesc desc;
    desc.deviceType = type;
    desc.slang.slangGlobalSession = dream::slang_replay::globalSession();
    desc.debugCallback = &device->sink;
    Result result = getRHI()->createDevice(desc, device->device.writeRef());
    if (SLANG_FAILED(result) || !device->device)
    {
        std::string detail = device->sink.takeLastError();
        delete device;
        return fail(
            DGPU_UNSUPPORTED,
            detail.empty() ? std::string("no adapter supports this backend on this machine") : detail
        );
    }
    result = device->device->getQueue(QueueType::Graphics, device->queue.writeRef());
    if (SLANG_SUCCEEDED(result))
    {
        FenceDesc fenceDesc;
        result = device->device->createFence(fenceDesc, device->fence.writeRef());
    }
    if (SLANG_FAILED(result))
    {
        int status = failResult(device, result, "device setup failed");
        delete device;
        return status;
    }
    *out = device;
    return DGPU_OK;
}

extern "C" void dgpu_device_destroy(DgpuDevice* device)
{
    if (!device)
        return;
    if (device->queue)
        device->queue->waitOnHost();
    delete device;
}

extern "C" uint32_t dgpu_device_features(DgpuDevice* device)
{
    IDevice* d = device->device;
    // GpuFeature order: RayTracing, RayQuery, Bindless, Subgroups, CooperativeVectors,
    // CooperativeMatrices, Float16, Atomics64.
    uint32_t mask = 0;
    auto set = [&](int bit, bool on)
    {
        if (on)
            mask |= 1u << bit;
    };
    set(0, d->hasFeature(Feature::RayTracing));
    set(1, d->hasFeature(Feature::RayQuery));
    set(2, d->hasFeature(Feature::Bindless));
    // slang-rhi's Metal device never reports WaveOps, but every GPU it opens (Metal 3) has the
    // SIMD-group functions Slang lowers subgroup operations to.
    set(3, d->hasFeature(Feature::WaveOps) || device->backend == 4);
    bool coopVecBackend = device->backend == 1 || device->backend == 3 || device->backend == 5;
    set(4, coopVecBackend && d->hasFeature(Feature::CooperativeVector));
    set(5, d->hasFeature(Feature::CooperativeMatrix));
    // Likewise, slang-rhi's Metal device never reports Half, which every Apple GPU has.
    set(6, d->hasFeature(Feature::Half) || device->backend == 4);
    set(7, d->hasFeature(Feature::AtomicInt64));
    return mask;
}

extern "C" int dgpu_device_lost(DgpuDevice* device)
{
    std::string reason;
    if (!device->sink.lost(&reason))
        return 0;
    fail(DGPU_DEVICE_LOST, reason);
    return 1;
}

extern "C" int dgpu_fence_wait(DgpuDevice* device, uint64_t value, uint64_t timeout_ns)
{
    IFence* fence = device->fence;
    Result result = device->device->waitForFences(1, &fence, &value, true, timeout_ns);
    if (result == SLANG_E_TIME_OUT)
        return DGPU_TIMEOUT;
    if (SLANG_FAILED(result))
        return failResult(device, result, "waiting for the GPU failed");
    return DGPU_OK;
}

extern "C" void dgpu_object_release(DgpuObject* object)
{
    if (object)
        static_cast<ISlangUnknown*>(object)->release();
}

extern "C" int dgpu_loaded_slang_compilers(void)
{
    std::vector<std::string> images = slangCompilerImages();
    if (!images.empty())
        fail(DGPU_FAILED, images.front());
    return int(images.size());
}
