#pragma once

// Reader for the probe's shader bundle (format written by build.py, see `write_bundle`).

#include <cstdint>
#include <string>
#include <vector>

namespace dream::gpu {

struct BundleEntryPoint
{
    std::string name;
    std::string stage;
    std::vector<uint8_t> code;
};

struct BundleTarget
{
    // slangc target name: "spirv", "metal", "wgsl", "dxil", "dxbc", "ptx".
    std::string target;
    std::vector<uint8_t> layout;
    std::vector<BundleEntryPoint> entryPoints;
    std::vector<std::string> capabilities;
};

struct Bundle
{
    std::vector<BundleTarget> targets;

    const BundleTarget* find(const std::string& target) const;
    static bool load(const std::string& path, Bundle& out, std::string& error);
};

} // namespace dream::gpu
