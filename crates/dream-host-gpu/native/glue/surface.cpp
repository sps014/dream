#include "glue/internal.h"

#if defined(__APPLE__)
#include <objc/message.h>
#include <objc/runtime.h>
#endif

using namespace rhi;
using namespace dream::gpu;

struct DgpuSurface
{
    DgpuDevice* device = nullptr;
    ComPtr<ISurface> surface;
};

namespace {

bool windowHandle(const DgpuWindow& window, WindowHandle& out)
{
    switch (window.platform)
    {
#if defined(__APPLE__)
    case 1:
    {
        // The window host hands out the content NSView; slang-rhi attaches its layer through the
        // owning NSWindow.
        using Send = void* (*)(void*, SEL);
        void* nswindow = reinterpret_cast<Send>(objc_msgSend)(window.window, sel_registerName("window"));
        if (!nswindow)
            return false;
        out = WindowHandle::fromNSWindow(nswindow);
        return true;
    }
#endif
#if defined(_WIN32)
    case 2:
        out = WindowHandle::fromHwnd(window.window);
        return true;
#endif
#if defined(__linux__)
    case 3:
        out = WindowHandle::fromXlibWindow(window.display, uint32_t(window.xlib_window));
        return true;
#endif
    default:
        return false;
    }
}

} // namespace

extern "C" int dgpu_surface_create(DgpuDevice* device, const DgpuWindow* window, DgpuSurface** out)
{
    *out = nullptr;
    WindowHandle handle;
    if (!windowHandle(*window, handle))
        return fail(DGPU_UNSUPPORTED, "the window system has no GPU surface support in this host");
    ComPtr<ISurface> surface;
    Result result = device->device->createSurface(handle, surface.writeRef());
    if (SLANG_FAILED(result) || !surface)
    {
        int status = failResult(device, result, "surface creation failed");
        return status == DGPU_DEVICE_LOST ? status : fail(DGPU_SURFACE_LOST, dgpu_message());
    }
    auto s = new DgpuSurface();
    s->device = device;
    s->surface = surface;
    *out = s;
    return DGPU_OK;
}

extern "C" int dgpu_surface_configure(DgpuSurface* s, uint32_t width, uint32_t height, int vsync, uint32_t* format)
{
    const SurfaceInfo& info = s->surface->getInfo();
    Format chosen = Format::Undefined;
    // Dream surfaces present 8-bit RGBA or BGRA, linear or sRGB (GpuTextureFormat 0 to 3).
    if (dreamTextureFormat(info.preferredFormat) < 4)
        chosen = info.preferredFormat;
    for (uint32_t i = 0; chosen == Format::Undefined && i < info.formatCount; i++)
        if (dreamTextureFormat(info.formats[i]) < 4)
            chosen = info.formats[i];
    if (chosen == Format::Undefined)
        return fail(DGPU_SURFACE_LOST, "the surface offers no 8-bit RGBA or BGRA format");
    SurfaceConfig config;
    config.format = chosen;
    config.width = width;
    config.height = height;
    config.vsync = vsync != 0;
    Result result = s->surface->configure(config);
    if (SLANG_FAILED(result))
    {
        int status = failResult(s->device, result, "surface configuration failed");
        return status == DGPU_DEVICE_LOST ? status : fail(DGPU_SURFACE_LOST, dgpu_message());
    }
    *format = dreamTextureFormat(chosen);
    return DGPU_OK;
}

extern "C" int dgpu_surface_acquire(DgpuSurface* s, DgpuObject** texture)
{
    *texture = nullptr;
    ComPtr<ITexture> image;
    Result result = s->surface->acquireNextImage(image.writeRef());
    if (SLANG_FAILED(result) || !image)
    {
        int status = failResult(s->device, result, "acquiring the next surface image failed");
        return status == DGPU_DEVICE_LOST ? status : fail(DGPU_SURFACE_LOST, dgpu_message());
    }
    *texture = image.detach();
    return DGPU_OK;
}

extern "C" int dgpu_surface_present(DgpuSurface* s)
{
    Result result = s->surface->present();
    if (SLANG_FAILED(result))
    {
        int status = failResult(s->device, result, "presenting failed");
        return status == DGPU_DEVICE_LOST ? status : fail(DGPU_SURFACE_LOST, dgpu_message());
    }
    return DGPU_OK;
}

extern "C" void dgpu_surface_release(DgpuSurface* s)
{
    delete s;
}
