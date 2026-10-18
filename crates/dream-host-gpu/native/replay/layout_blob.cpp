#include "replay/layout_blob.h"

#include <cstring>

namespace dream::slang_replay {

namespace {

void putU32(std::vector<uint8_t>& out, uint32_t v)
{
    for (int i = 0; i < 4; i++)
        out.push_back(uint8_t(v >> (8 * i)));
}

void putI64(std::vector<uint8_t>& out, int64_t v)
{
    uint64_t u = uint64_t(v);
    for (int i = 0; i < 8; i++)
        out.push_back(uint8_t(u >> (8 * i)));
}

struct Reader
{
    const uint8_t* data;
    size_t size;
    size_t pos = 0;
    bool ok = true;

    bool need(size_t n)
    {
        if (!ok || size - pos < n)
            ok = false;
        return ok;
    }
    uint32_t u32()
    {
        if (!need(4))
            return 0;
        uint32_t v = 0;
        for (int i = 0; i < 4; i++)
            v |= uint32_t(data[pos + i]) << (8 * i);
        pos += 4;
        return v;
    }
    int64_t i64()
    {
        if (!need(8))
            return 0;
        uint64_t v = 0;
        for (int i = 0; i < 8; i++)
            v |= uint64_t(data[pos + i]) << (8 * i);
        pos += 8;
        return int64_t(v);
    }
    uint8_t u8()
    {
        if (!need(1))
            return 0;
        return data[pos++];
    }
};

} // namespace

std::vector<uint8_t> LayoutBlob::serialize() const
{
    std::vector<uint8_t> out;
    out.insert(out.end(), kLayoutMagic, kLayoutMagic + 4);
    putU32(out, kLayoutVersion);

    putU32(out, uint32_t(nodes.size()));
    for (NodeKind kind : nodes)
        out.push_back(uint8_t(kind));

    putU32(out, uint32_t(strings.size()));
    for (const std::string& s : strings)
    {
        putU32(out, uint32_t(s.size()));
        out.insert(out.end(), s.begin(), s.end());
    }

    putU32(out, uint32_t(entries.size()));
    for (const Entry& e : entries)
    {
        putU32(out, e.node);
        putU32(out, uint32_t(e.query));
        putU32(out, uint32_t(e.arg0));
        putU32(out, uint32_t(e.arg1));
        out.push_back(uint8_t(e.value.kind));
        putI64(out, e.value.payload);
    }
    return out;
}

bool LayoutBlob::deserialize(const uint8_t* data, size_t size, LayoutBlob& out, std::string& error)
{
    Reader r{data, size};
    if (size < 8 || std::memcmp(data, kLayoutMagic, 4) != 0)
    {
        error = "not a Slang layout blob";
        return false;
    }
    r.pos = 4;
    if (r.u32() != kLayoutVersion)
    {
        error = "unsupported Slang layout blob version";
        return false;
    }

    uint32_t nodeCount = r.u32();
    if (!r.need(nodeCount))
    {
        error = "truncated node table";
        return false;
    }
    out.nodes.resize(nodeCount);
    for (uint32_t i = 0; i < nodeCount; i++)
    {
        uint8_t kind = r.u8();
        if (kind > uint8_t(NodeKind::Type))
        {
            error = "unknown node kind";
            return false;
        }
        out.nodes[i] = NodeKind(kind);
    }

    uint32_t stringCount = r.u32();
    for (uint32_t i = 0; i < stringCount && r.ok; i++)
    {
        uint32_t len = r.u32();
        if (!r.need(len))
            break;
        out.strings.emplace_back(reinterpret_cast<const char*>(data + r.pos), len);
        r.pos += len;
    }

    uint32_t entryCount = r.u32();
    for (uint32_t i = 0; i < entryCount && r.ok; i++)
    {
        Entry e;
        e.node = r.u32();
        e.query = Query(r.u32());
        e.arg0 = int32_t(r.u32());
        e.arg1 = int32_t(r.u32());
        e.value.kind = ValueKind(r.u8());
        e.value.payload = r.i64();
        bool valid = e.node < nodeCount && uint8_t(e.value.kind) <= uint8_t(ValueKind::String);
        if (e.value.kind == ValueKind::Node)
            valid = valid && e.value.payload >= 0 && uint64_t(e.value.payload) < nodeCount;
        if (e.value.kind == ValueKind::String)
            valid = valid && e.value.payload >= 0 && uint64_t(e.value.payload) < out.strings.size();
        if (!valid)
        {
            error = "layout entry references a missing node or string";
            return false;
        }
        out.entries.push_back(e);
    }

    if (!r.ok || r.pos != size || nodeCount == 0 || out.nodes[0] != NodeKind::Program)
    {
        error = "malformed Slang layout blob";
        return false;
    }
    return true;
}

} // namespace dream::slang_replay
