// Milestone 0 probe: dispatch the precompiled saxpy kernel through slang-rhi on each requested
// backend, using only the bundle's target code and recorded layout, then assert that no Slang
// compiler image was ever mapped into the process.
//
// usage: aot_probe <bundle> <backend>...      backends: metal vulkan wgpu d3d12 d3d11 cuda
//        aot_probe --loader-self-test <path-to-libslang-compiler>

#include "bundle.h"
#include "loader_check.h"
#include "replay/replay.h"

#include <slang-rhi.h>
#include <slang-rhi/shader-cursor.h>

#include <cmath>
#include <cstdio>
#include <cstring>
#include <map>
#include <string>
#include <vector>

#if !defined(_WIN32)
#include <dlfcn.h>
#endif

using namespace rhi;
using dream::gpu::Bundle;
using dream::gpu::BundleTarget;

namespace {

class DebugPrinter : public IDebugCallback
{
public:
    SLANG_NO_THROW void SLANG_MCALL
    handleMessage(DebugMessageType type, DebugMessageSource source, const char* message) override
    {
        if (type == DebugMessageType::Info)
            return;
        const char* origin = source == DebugMessageSource::Slang    ? "slang"
                             : source == DebugMessageSource::Driver ? "driver"
                                                                    : "rhi";
        std::fprintf(stderr, "    [%s] %s\n", origin, message);
    }
};

struct BackendInfo
{
    DeviceType type;
    const char* target;
};

const std::map<std::string, BackendInfo> kBackends = {
    {"metal", {DeviceType::Metal, "metal"}},
    {"vulkan", {DeviceType::Vulkan, "spirv"}},
    {"wgpu", {DeviceType::WGPU, "wgsl"}},
    {"d3d12", {DeviceType::D3D12, "dxil"}},
    {"d3d11", {DeviceType::D3D11, "dxbc"}},
    {"cuda", {DeviceType::CUDA, "ptx"}},
};

struct Outcome
{
    std::string status;
    std::string detail;
};

struct SaxpyParams
{
    float a;
    int32_t n;
};

Outcome runSaxpy(const std::string& backend, const Bundle& bundle, DebugPrinter& printer)
{
    auto it = kBackends.find(backend);
    if (it == kBackends.end())
        return {"error", "unknown backend"};
    const BackendInfo& info = it->second;

    const BundleTarget* target = bundle.find(info.target);
    if (!target)
        return {"not-in-bundle", std::string("bundle has no ") + info.target + " code"};
    if (!getRHI()->isDeviceTypeSupported(info.type))
        return {"not-available", "slang-rhi was built without this backend"};

    DeviceDesc deviceDesc;
    deviceDesc.deviceType = info.type;
    deviceDesc.slang.slangGlobalSession = dream::slang_replay::globalSession();
    deviceDesc.debugCallback = &printer;
    ComPtr<IDevice> device;
    if (SLANG_FAILED(getRHI()->createDevice(deviceDesc, device.writeRef())))
        return {"not-available", "no device for this backend on this machine"};
    std::string adapter = device->getInfo().adapterName ? device->getInfo().adapterName : "?";
    uint32_t coopVecProperties = 0;
    device->getCooperativeVectorProperties(nullptr, &coopVecProperties);
    std::printf(
        "FEATURE %s cooperative-vector=%d cooperative-matrix=%d coop-vec-property-count=%u\n",
        backend.c_str(),
        int(device->hasFeature(Feature::CooperativeVector)),
        int(device->hasFeature(Feature::CooperativeMatrix)),
        coopVecProperties
    );

    std::vector<dream::slang_replay::PrecompiledEntryPoint> entryPoints;
    for (const auto& entry : target->entryPoints)
        entryPoints.push_back({entry.name.c_str(), entry.code.data(), entry.code.size()});
    ComPtr<slang::IComponentType> component;
    std::string error;
    if (SLANG_FAILED(dream::slang_replay::createPrecompiledProgram(
            target->layout.data(),
            target->layout.size(),
            entryPoints.data(),
            uint32_t(entryPoints.size()),
            component.writeRef(),
            &error
        )))
        return {"fail", "createPrecompiledProgram: " + error};

    ShaderProgramDesc programDesc;
    programDesc.slangGlobalScope = component;
    programDesc.label = "saxpy";
    ComPtr<IShaderProgram> program;
    if (SLANG_FAILED(device->createShaderProgram(programDesc, program.writeRef())))
        return {"fail", "createShaderProgram failed"};

    ComputePipelineDesc pipelineDesc;
    pipelineDesc.program = program;
    ComPtr<IComputePipeline> pipeline;
    if (SLANG_FAILED(device->createComputePipeline(pipelineDesc, pipeline.writeRef())))
        return {"fail", "createComputePipeline failed"};

    // Not a multiple of the 64-wide workgroup, so the bounds check is exercised.
    const int32_t n = 1000;
    const float a = 2.5f;
    std::vector<float> x(n), y(n), result(n, -1.0f);
    for (int32_t i = 0; i < n; i++)
    {
        x[i] = float(i) * 0.5f;
        y[i] = 100.0f - float(i);
    }

    BufferDesc inputDesc;
    inputDesc.size = sizeof(float) * n;
    inputDesc.elementSize = sizeof(float);
    inputDesc.usage = BufferUsage::ShaderResource | BufferUsage::CopyDestination;
    inputDesc.defaultState = ResourceState::ShaderResource;
    ComPtr<IBuffer> xBuffer = device->createBuffer(inputDesc, x.data());
    ComPtr<IBuffer> yBuffer = device->createBuffer(inputDesc, y.data());

    BufferDesc outputDesc = inputDesc;
    outputDesc.usage = BufferUsage::UnorderedAccess | BufferUsage::CopySource | BufferUsage::CopyDestination;
    outputDesc.defaultState = ResourceState::UnorderedAccess;
    ComPtr<IBuffer> outBuffer = device->createBuffer(outputDesc, result.data());
    if (!xBuffer || !yBuffer || !outBuffer)
        return {"fail", "buffer creation failed"};

    ComPtr<ICommandQueue> queue = device->getQueue(QueueType::Graphics);
    ComPtr<ICommandEncoder> encoder = queue->createCommandEncoder();
    IComputePassEncoder* pass = encoder->beginComputePass();
    IShaderObject* root = pass->bindPipeline(pipeline);
    ShaderCursor cursor(root);
    bool bound = SLANG_SUCCEEDED(cursor["x"].setBinding(xBuffer)) &&
                 SLANG_SUCCEEDED(cursor["y"].setBinding(yBuffer)) &&
                 SLANG_SUCCEEDED(cursor["result"].setBinding(outBuffer)) &&
                 SLANG_SUCCEEDED(cursor["params"]["a"].setData(a)) &&
                 SLANG_SUCCEEDED(cursor["params"]["n"].setData(n));
    if (!bound)
    {
        pass->end();
        return {"fail", "binding by name through the replayed layout failed"};
    }
    pass->dispatchCompute(uint32_t((n + 63) / 64), 1, 1);
    pass->end();
    if (SLANG_FAILED(queue->submit(encoder->finish())))
        return {"fail", "submit failed"};
    queue->waitOnHost();

    if (SLANG_FAILED(device->readBuffer(outBuffer, 0, sizeof(float) * n, result.data())))
        return {"fail", "readback failed"};
    for (int32_t i = 0; i < n; i++)
    {
        float expected = a * x[i] + y[i];
        if (std::fabs(result[i] - expected) > 1e-4f)
        {
            char buf[160];
            std::snprintf(buf, sizeof(buf), "result[%d] = %g, expected %g", i, result[i], expected);
            return {"fail", buf};
        }
    }
    return {"pass", adapter + ", " + std::to_string(n) + " elements verified"};
}

int loaderSelfTest(const char* compilerPath)
{
#if defined(_WIN32)
    (void)compilerPath;
    std::printf("loader self-test not implemented on Windows\n");
    return 0;
#else
    dream::gpu::watchLoadedImages();
    if (!dream::gpu::slangCompilerImages().empty())
    {
        std::printf("loader self-test: compiler present before dlopen\n");
        return 1;
    }
    void* handle = dlopen(compilerPath, RTLD_NOW | RTLD_LOCAL);
    if (!handle)
    {
        std::printf("loader self-test: dlopen failed: %s\n", dlerror());
        return 1;
    }
    auto hits = dream::gpu::slangCompilerImages();
    std::printf("loader self-test: detected %zu compiler image(s) after dlopen\n", hits.size());
    return hits.empty() ? 1 : 0;
#endif
}

} // namespace

int main(int argc, char** argv)
{
    if (argc == 3 && std::strcmp(argv[1], "--loader-self-test") == 0)
        return loaderSelfTest(argv[2]);
    if (argc < 3)
    {
        std::fprintf(stderr, "usage: aot_probe <bundle> <backend>...\n");
        return 2;
    }
    dream::gpu::watchLoadedImages();

    Bundle bundle;
    std::string error;
    if (!Bundle::load(argv[1], bundle, error))
    {
        std::fprintf(stderr, "%s\n", error.c_str());
        return 2;
    }

    DebugPrinter printer;
    bool failed = false;
    for (int i = 2; i < argc; i++)
    {
        Outcome outcome = runSaxpy(argv[i], bundle, printer);
        std::printf("RESULT %s %s: %s\n", argv[i], outcome.status.c_str(), outcome.detail.c_str());
        std::fflush(stdout);
        failed |= outcome.status == "fail" || outcome.status == "error";
    }

    auto compilerImages = dream::gpu::slangCompilerImages();
    for (const std::string& image : compilerImages)
        std::printf("LOADER slang compiler image mapped: %s\n", image.c_str());
    std::printf("LOADER %s: %zu images mapped, %zu Slang compiler images\n",
                compilerImages.empty() ? "pass" : "fail",
                dream::gpu::loadedImages().size(),
                compilerImages.size());
    return failed || !compilerImages.empty() ? 1 : 0;
}
