#pragma once

#include "replay/layout_blob.h"

#include <slang.h>

#include <memory>
#include <string>
#include <unordered_map>
#include <vector>

namespace dream::slang_replay {

class ReplayLayout;

struct ReplayNode
{
    NodeKind kind = NodeKind::Program;
    ReplayLayout* owner = nullptr;
    std::unordered_map<uint64_t, Value> answers;
};

class ReplayLayout
{
public:
    static std::shared_ptr<ReplayLayout> load(const uint8_t* data, size_t size, std::string& error);

    ReplayNode* program() { return &m_nodes[0]; }

    /// Re-points the program's entry point list at the recorded entry points named `names`, in
    /// that order. Returns false and names the first unknown entry in `error`.
    bool selectEntryPoints(const std::vector<std::string>& names, std::string& error);
    ReplayNode* typeByName(const char* name) const;

    int64_t integer(const ReplayNode* node, Query query, int64_t arg0 = 0, int64_t arg1 = 0) const;
    ReplayNode* node(const ReplayNode* node, Query query, int64_t arg0 = 0, int64_t arg1 = 0);
    const char* string(const ReplayNode* node, Query query, int64_t arg0 = 0, int64_t arg1 = 0) const;

    // Lets `ISession::getTypeLayout` answer for the program that owns the session.
    slang::ISession* session = nullptr;

private:
    static uint64_t key(Query query, int64_t arg0, int64_t arg1);
    const Value* find(const ReplayNode* node, Query query, int64_t arg0, int64_t arg1) const;

    std::vector<ReplayNode> m_nodes;
    std::vector<std::string> m_strings;
    std::unordered_map<std::string, uint32_t> m_typesByName;
};

template<typename T>
ReplayNode* asNode(T* reflection)
{
    return reinterpret_cast<ReplayNode*>(reflection);
}

template<typename T>
T* asReflection(ReplayNode* node)
{
    return reinterpret_cast<T*>(node);
}

} // namespace dream::slang_replay
