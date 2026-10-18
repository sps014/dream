#include "bundle.h"

#include <cstring>
#include <fstream>
#include <iterator>

namespace dream::gpu {

namespace {

struct Reader
{
    const std::vector<uint8_t>& bytes;
    size_t pos = 0;
    bool ok = true;

    bool need(size_t n)
    {
        if (!ok || bytes.size() - pos < n)
            ok = false;
        return ok;
    }
    uint32_t u32()
    {
        if (!need(4))
            return 0;
        uint32_t v = 0;
        for (int i = 0; i < 4; i++)
            v |= uint32_t(bytes[pos + i]) << (8 * i);
        pos += 4;
        return v;
    }
    std::string str()
    {
        uint32_t len = u32();
        if (!need(len))
            return {};
        std::string s(reinterpret_cast<const char*>(bytes.data() + pos), len);
        pos += len;
        return s;
    }
    // Blobs are (offset, size) into the data section; their BLAKE3 digest follows.
    std::vector<uint8_t> blob(size_t dataStart)
    {
        uint32_t offset = u32();
        uint32_t size = u32();
        if (!need(32))
            return {};
        pos += 32;
        if (dataStart > bytes.size() || offset > bytes.size() - dataStart ||
            size > bytes.size() - dataStart - offset)
        {
            ok = false;
            return {};
        }
        auto begin = bytes.begin() + std::ptrdiff_t(dataStart + offset);
        return std::vector<uint8_t>(begin, begin + size);
    }
};

} // namespace

const BundleTarget* Bundle::find(const std::string& target) const
{
    for (const BundleTarget& t : targets)
        if (t.target == target)
            return &t;
    return nullptr;
}

bool Bundle::load(const std::string& path, Bundle& out, std::string& error)
{
    std::ifstream in(path, std::ios::binary);
    if (!in)
    {
        error = "cannot open " + path;
        return false;
    }
    std::vector<uint8_t> bytes((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
    if (bytes.size() < 16 || std::memcmp(bytes.data(), "DGPB", 4) != 0)
    {
        error = "not a Dream GPU bundle";
        return false;
    }
    Reader r{bytes, 4};
    if (r.u32() != 0)
    {
        error = "unsupported bundle version";
        return false;
    }
    uint32_t headerSize = r.u32();
    size_t dataStart = headerSize;
    uint32_t targetCount = r.u32();
    for (uint32_t t = 0; t < targetCount && r.ok; t++)
    {
        BundleTarget target;
        target.target = r.str();
        target.layout = r.blob(dataStart);
        uint32_t entryCount = r.u32();
        for (uint32_t e = 0; e < entryCount && r.ok; e++)
        {
            BundleEntryPoint entry;
            entry.name = r.str();
            entry.stage = r.str();
            entry.code = r.blob(dataStart);
            target.entryPoints.push_back(std::move(entry));
        }
        uint32_t capabilityCount = r.u32();
        for (uint32_t c = 0; c < capabilityCount && r.ok; c++)
            target.capabilities.push_back(r.str());
        out.targets.push_back(std::move(target));
    }
    if (!r.ok || r.pos != headerSize)
    {
        error = "malformed bundle header";
        return false;
    }
    return true;
}

} // namespace dream::gpu
