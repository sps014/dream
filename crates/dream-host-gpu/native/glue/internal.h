#pragma once

#include "glue/dgpu.h"

#include <slang-rhi.h>

#include <algorithm>
#include <atomic>
#include <iterator>
#include <memory>
#include <mutex>
#include <string>
#include <utility>
#include <vector>

namespace dream::gpu {

using rhi::ComPtr;

/// Records `message` as the calling thread's failure detail and returns `status`.
int fail(int status, std::string message);
/// Maps a failed slang-rhi result to a status, preferring the device's sticky loss.
int failResult(DgpuDevice* device, rhi::Result result, const char* what);

/// Captures driver messages: the last error explains the next failed call, and loss reports
/// make the device sticky-lost.
class DebugSink final : public rhi::IDebugCallback
{
public:
    SLANG_NO_THROW void SLANG_MCALL
    handleMessage(rhi::DebugMessageType type, rhi::DebugMessageSource source, const char* message) override;

    /// The first error reported since the previous call. Later errors are usually consequences of
    /// it (a failed pipeline creation is followed by "invalid pipeline" when it is used).
    std::string takeFirstError();
    bool lost(std::string* reason);

private:
    std::mutex m_mutex;
    std::string m_firstError;
    std::string m_lostReason;
    std::atomic<bool> m_lost{false};
};

/// One resolved bundle parameter: the cursor path slang-rhi binds through.
struct BindingPath
{
    /// -1 for the root (global) object, else the program entry point index.
    int object = -1;
    /// Field of that object's element struct, or empty to address the object itself.
    std::string field;
    /// Steps through a ConstantBuffer / ParameterBlock to its element.
    bool dereference = false;
    /// Added to the cursor's uniform offset (value parameters).
    uint32_t uniformOffset = 0;
    int32_t kind = 0;
    uint32_t size = 0;
};

struct DgpuProgramData;

} // namespace dream::gpu

struct DgpuDevice
{
    int backend = 0;
    rhi::ComPtr<rhi::IDevice> device;
    rhi::ComPtr<rhi::ICommandQueue> queue;
    rhi::ComPtr<rhi::IFence> fence;
    uint64_t nextFenceValue = 0;
    dream::gpu::DebugSink sink;
    /// Stand-ins for slots no shader of the pipeline uses (WebGPU needs every slot bound).
    /// Read-only and writable slots get separate stand-ins: WebGPU rejects one resource used
    /// both ways in a pass. Each writable slot gets its own stand-in, because WebGPU also
    /// rejects one buffer or texture bound to two writable slots (aliasing).
    rhi::ComPtr<rhi::IBuffer> placeholderBuffer;
    std::vector<rhi::ComPtr<rhi::IBuffer>> placeholderMutableBuffers;
    rhi::ComPtr<rhi::ITexture> placeholderTexture;
    std::vector<rhi::ComPtr<rhi::ITexture>> placeholderMutableTextures;
    rhi::ComPtr<rhi::ISampler> placeholderSampler;
};

struct DgpuProgram
{
    DgpuDevice* device = nullptr;
    rhi::ComPtr<slang::IComponentType> component;
    rhi::ComPtr<rhi::IShaderProgram> program;
};

struct DgpuPipeline
{
    bool render = false;
    bool rayTracing = false;
    rhi::ComPtr<rhi::IComputePipeline> compute;
    rhi::ComPtr<rhi::IRenderPipeline> graphics;
    rhi::ComPtr<rhi::IRayTracingPipeline> rayTracingPipeline;
    rhi::ComPtr<rhi::IShaderTable> shaderTable;
    /// Bindings per program entry point (compute: 1, render: vertex then fragment).
    std::vector<std::vector<dream::gpu::BindingPath>> stages;
};

struct DgpuEncoder
{
    DgpuDevice* device = nullptr;
    rhi::ComPtr<rhi::ICommandEncoder> encoder;
    rhi::IRenderPassEncoder* render = nullptr;
    rhi::IComputePassEncoder* compute = nullptr;
    rhi::IRayTracingPassEncoder* rayTracing = nullptr;
    DgpuPipeline* pipeline = nullptr;
    rhi::IShaderObject* root = nullptr;
    rhi::RenderState state;
    bool viewportSet = false;
    bool scissorSet = false;
    uint32_t targetWidth = 0;
    uint32_t targetHeight = 0;
};

namespace dream::gpu {

rhi::DeviceType deviceType(int backend);
rhi::Format textureFormat(uint32_t format);
uint32_t dreamTextureFormat(rhi::Format format);

/// Binds placeholders into every resource slot of `object` and its sub-objects. A module's
/// layout covers the globals of all its entry points; WebGPU rejects a bind group with any
/// slot empty, while the other backends ignore slots the code never reads.
int fillUnboundSlots(DgpuDevice* device, rhi::IShaderObject* object);

/// Resolves each stage's bundle parameters against the program's recorded layout.
int resolveBindings(
    DgpuProgram* program,
    const DgpuStageParams* stages,
    uint32_t stageCount,
    std::vector<std::vector<BindingPath>>& out
);

} // namespace dream::gpu
