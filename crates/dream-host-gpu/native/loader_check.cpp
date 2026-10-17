#include "loader_check.h"

#include <algorithm>
#include <mutex>
#include <set>

#if defined(__APPLE__)
#include <mach-o/dyld.h>
#elif defined(__linux__)
#include <link.h>
#elif defined(_WIN32)
#include <windows.h>
#include <psapi.h>
#endif

namespace dream::gpu {

namespace {

std::mutex g_mutex;
std::set<std::string> g_seen;

std::string fileName(const std::string& path)
{
    size_t slash = path.find_last_of("/\\");
    return slash == std::string::npos ? path : path.substr(slash + 1);
}

// libslang.* is the compiler's compatibility name; slang-rt and slang-rhi are not compilers.
bool isSlangCompiler(const std::string& path)
{
    std::string name = fileName(path);
    std::transform(name.begin(), name.end(), name.begin(), [](unsigned char c) { return char(std::tolower(c)); });
    const char* markers[] = {
        "slang-compiler",
        "libslang.",
        "slang.dll",
        "slang-glslang",
        "slang-llvm",
        "slang-glsl-module",
        "slang-standard-module",
    };
    for (const char* marker : markers)
        if (name.find(marker) != std::string::npos)
            return true;
    return false;
}

void remember(const char* path)
{
    if (!path)
        return;
    std::lock_guard<std::mutex> lock(g_mutex);
    g_seen.insert(path);
}

#if defined(__APPLE__)
void onAddImage(const struct mach_header* header, intptr_t)
{
    for (uint32_t i = 0; i < _dyld_image_count(); i++)
        if (_dyld_get_image_header(i) == header)
            remember(_dyld_get_image_name(i));
}
#endif

} // namespace

void watchLoadedImages()
{
#if defined(__APPLE__)
    static std::once_flag once;
    std::call_once(once, [] { _dyld_register_func_for_add_image(onAddImage); });
#endif
}

std::vector<std::string> loadedImages()
{
    std::vector<std::string> images;
#if defined(__APPLE__)
    for (uint32_t i = 0; i < _dyld_image_count(); i++)
        if (const char* name = _dyld_get_image_name(i))
            images.emplace_back(name);
#elif defined(__linux__)
    dl_iterate_phdr(
        [](struct dl_phdr_info* info, size_t, void* data) -> int
        {
            if (info->dlpi_name && info->dlpi_name[0])
                static_cast<std::vector<std::string>*>(data)->emplace_back(info->dlpi_name);
            return 0;
        },
        &images
    );
#elif defined(_WIN32)
    HMODULE modules[1024];
    DWORD needed = 0;
    if (EnumProcessModules(GetCurrentProcess(), modules, sizeof(modules), &needed))
    {
        for (DWORD i = 0; i < needed / sizeof(HMODULE); i++)
        {
            char name[MAX_PATH];
            if (GetModuleFileNameA(modules[i], name, MAX_PATH))
                images.emplace_back(name);
        }
    }
#endif
    return images;
}

std::vector<std::string> slangCompilerImages()
{
    std::set<std::string> all;
    {
        std::lock_guard<std::mutex> lock(g_mutex);
        all = g_seen;
    }
    for (const std::string& image : loadedImages())
        all.insert(image);
    std::vector<std::string> hits;
    for (const std::string& image : all)
        if (isSlangCompiler(image))
            hits.push_back(image);
    return hits;
}

} // namespace dream::gpu
