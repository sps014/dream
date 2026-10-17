// dream-slang-layout: the build-time half of strict ahead-of-time shader loading. It compiles a
// Slang module with the real compiler, walks the linked program's reflection and writes every
// answer the native replay library can be asked for (docs/internals/18-gpu-shader-bundle.md).
//
// usage: dream-slang-layout <module.slang>... [-entry <name> [-stage <stage>] [-o <code>]]...
//            <slangc options> -target <target> -o <layout.bin>
//
// Options are slangc's. Positional `.slang` files are the modules; `-entry` names select entry
// points in bundle order (default: every `[shader(...)]` entry in declaration order). The last
// `-o` names the layout; as in slangc, an earlier `-o` right after an entry (or its `-stage`)
// names that entry's code, written from the same linked program the layout describes. slangc
// itself cannot write per-entry code for several entries in one run.
// `-reflection-json <file>` is accepted and ignored so one option list serves both tools;
// everything else configures the compile session exactly as slangc would.

#include "replay/layout_blob.h"

#include <slang-com-ptr.h>
#include <slang.h>

#include <cstdio>
#include <cstring>
#include <deque>
#include <fstream>
#include <map>
#include <string>
#include <vector>

using namespace dream::slang_replay;
using Slang::ComPtr;

namespace {

class Recorder
{
public:
    Recorder(slang::ISession* session, slang::ProgramLayout* program, bool metal)
        : m_session(session)
        , m_program(program)
        , m_metal(metal)
    {
    }

    LayoutBlob run()
    {
        intern(NodeKind::Program, m_program);
        while (!m_work.empty())
        {
            auto [kind, ptr] = m_work.front();
            m_work.pop_front();
            uint32_t id = m_ids.at({kind, ptr});
            switch (kind)
            {
            case NodeKind::Program:
                program(id, static_cast<slang::ProgramLayout*>(ptr));
                break;
            case NodeKind::EntryPoint:
                entryPoint(id, static_cast<slang::EntryPointReflection*>(ptr));
                break;
            case NodeKind::VariableLayout:
                variableLayout(id, static_cast<slang::VariableLayoutReflection*>(ptr));
                break;
            case NodeKind::TypeLayout:
                typeLayout(id, static_cast<slang::TypeLayoutReflection*>(ptr));
                break;
            case NodeKind::Type:
                type(id, static_cast<slang::TypeReflection*>(ptr));
                break;
            }
        }
        return std::move(m_blob);
    }

private:
    uint32_t intern(NodeKind kind, void* ptr)
    {
        auto [it, inserted] = m_ids.emplace(std::pair{kind, ptr}, uint32_t(m_blob.nodes.size()));
        if (inserted)
        {
            m_blob.nodes.push_back(kind);
            m_work.emplace_back(kind, ptr);
        }
        return it->second;
    }

    void put(uint32_t node, Query q, int64_t value, int32_t a0 = 0, int32_t a1 = 0)
    {
        if (value != 0)
            m_blob.entries.push_back({node, q, a0, a1, {ValueKind::Int, value}});
    }

    void putNode(uint32_t node, Query q, NodeKind kind, void* ptr, int32_t a0 = 0, int32_t a1 = 0)
    {
        if (ptr)
            m_blob.entries.push_back({node, q, a0, a1, {ValueKind::Node, int64_t(intern(kind, ptr))}});
    }

    void putString(uint32_t node, Query q, const char* s)
    {
        if (!s)
            return;
        auto [it, inserted] = m_strings.emplace(s, uint32_t(m_blob.strings.size()));
        if (inserted)
            m_blob.strings.emplace_back(s);
        m_blob.entries.push_back({node, q, 0, 0, {ValueKind::String, int64_t(it->second)}});
    }

    void program(uint32_t id, slang::ProgramLayout* p)
    {
        putNode(id, Query::ProgramGlobalParamsVarLayout, NodeKind::VariableLayout, p->getGlobalParamsVarLayout());
        SlangUInt count = p->getEntryPointCount();
        put(id, Query::ProgramEntryPointCount, int64_t(count));
        for (SlangUInt i = 0; i < count; i++)
            putNode(id, Query::ProgramEntryPointByIndex, NodeKind::EntryPoint, p->getEntryPointByIndex(i), int32_t(i));
    }

    void entryPoint(uint32_t id, slang::EntryPointReflection* e)
    {
        putString(id, Query::EntryPointName, e->getName());
        putString(id, Query::EntryPointNameOverride, e->getNameOverride());
        put(id, Query::EntryPointStage, e->getStage());
        putNode(id, Query::EntryPointVarLayout, NodeKind::VariableLayout, e->getVarLayout());
        SlangUInt size[3] = {};
        e->getComputeThreadGroupSize(3, size);
        for (int axis = 0; axis < 3; axis++)
            put(id, Query::EntryPointThreadGroupSize, int64_t(size[axis]), axis);
    }

    void variableLayout(uint32_t id, slang::VariableLayoutReflection* v)
    {
        putNode(id, Query::VariableLayoutTypeLayout, NodeKind::TypeLayout, v->getTypeLayout());
        putString(id, Query::VariableLayoutName, v->getName());
        putString(id, Query::VariableLayoutSemanticName, v->getSemanticName());
        put(id, Query::VariableLayoutSemanticIndex, int64_t(v->getSemanticIndex()));
        put(id, Query::VariableLayoutStage, v->getStage());
        for (int c = 0; c < SLANG_PARAMETER_CATEGORY_COUNT; c++)
        {
            auto category = slang::ParameterCategory(c);
            put(id, Query::VariableLayoutOffset, int64_t(v->getOffset(category)), c);
            put(id, Query::VariableLayoutSpace, int64_t(v->getBindingSpace(category)), c);
        }
    }

    void typeLayout(uint32_t id, slang::TypeLayoutReflection* t)
    {
        putNode(id, Query::TypeLayoutType, NodeKind::Type, t->getType());
        auto kind = t->getKind();
        put(id, Query::TypeLayoutKind, int64_t(kind));
        put(id, Query::TypeLayoutParameterCategory, int64_t(t->getParameterCategory()));
        put(id, Query::TypeLayoutMatrixLayoutMode, int64_t(t->getMatrixLayoutMode()));
        for (int c = 0; c < SLANG_PARAMETER_CATEGORY_COUNT; c++)
        {
            auto category = slang::ParameterCategory(c);
            put(id, Query::TypeLayoutSize, int64_t(t->getSize(category)), c);
            put(id, Query::TypeLayoutStride, int64_t(t->getStride(category)), c);
            put(id, Query::TypeLayoutAlignment, int64_t(t->getAlignment(category)), c);
            put(id, Query::TypeLayoutElementStride, int64_t(t->getElementStride(SlangParameterCategory(c))), c);
        }
        unsigned categoryCount = t->getCategoryCount();
        put(id, Query::TypeLayoutCategoryCount, categoryCount);
        for (unsigned i = 0; i < categoryCount; i++)
            put(id, Query::TypeLayoutCategoryByIndex, int64_t(t->getCategoryByIndex(i)), int32_t(i));

        unsigned fieldCount = t->getFieldCount();
        put(id, Query::TypeLayoutFieldCount, fieldCount);
        for (unsigned i = 0; i < fieldCount; i++)
        {
            putNode(id, Query::TypeLayoutFieldByIndex, NodeKind::VariableLayout, t->getFieldByIndex(i), int32_t(i));
            if (kind == slang::TypeReflection::Kind::Struct)
                put(id, Query::TypeLayoutFieldBindingRangeOffset, t->getFieldBindingRangeOffset(i), int32_t(i));
        }

        putNode(id, Query::TypeLayoutElementTypeLayout, NodeKind::TypeLayout, t->getElementTypeLayout());
        putNode(id, Query::TypeLayoutElementVarLayout, NodeKind::VariableLayout, t->getElementVarLayout());
        putNode(id, Query::TypeLayoutContainerVarLayout, NodeKind::VariableLayout, t->getContainerVarLayout());
        putNode(id, Query::TypeLayoutExplicitCounter, NodeKind::VariableLayout, t->getExplicitCounter());

        // slang-rhi's Metal backend re-queries a parameter block's element type under
        // argument-buffer rules. Asking for other types under those rules crashes the compiler.
        if (m_metal && kind == slang::TypeReflection::Kind::ParameterBlock)
        {
            slang::TypeLayoutReflection* element = t->getElementTypeLayout();
            slang::TypeReflection* elementType = element ? element->getType() : nullptr;
            if (elementType)
            {
                auto rules = slang::LayoutRules::MetalArgumentBufferTier2;
                putNode(
                    intern(NodeKind::Type, elementType),
                    Query::SessionTypeLayout,
                    NodeKind::TypeLayout,
                    m_session->getTypeLayout(elementType, 0, rules),
                    int32_t(rules)
                );
            }
        }

        SlangInt bindingRanges = t->getBindingRangeCount();
        put(id, Query::TypeLayoutBindingRangeCount, bindingRanges);
        for (SlangInt i = 0; i < bindingRanges; i++)
        {
            int32_t a = int32_t(i);
            put(id, Query::TypeLayoutBindingRangeType, int64_t(t->getBindingRangeType(i)), a);
            put(id, Query::TypeLayoutBindingRangeBindingCount, t->getBindingRangeBindingCount(i), a);
            put(id, Query::TypeLayoutBindingRangeDescriptorRangeCount, t->getBindingRangeDescriptorRangeCount(i), a);
            put(id, Query::TypeLayoutBindingRangeDescriptorSetIndex, t->getBindingRangeDescriptorSetIndex(i), a);
            put(id,
                Query::TypeLayoutBindingRangeFirstDescriptorRangeIndex,
                t->getBindingRangeFirstDescriptorRangeIndex(i),
                a);
            put(id, Query::TypeLayoutBindingRangeSpecializable, t->isBindingRangeSpecializable(i) ? 1 : 0, a);
            putNode(
                id,
                Query::TypeLayoutBindingRangeLeafTypeLayout,
                NodeKind::TypeLayout,
                t->getBindingRangeLeafTypeLayout(i),
                a
            );
        }

        SlangInt sets = t->getDescriptorSetCount();
        put(id, Query::TypeLayoutDescriptorSetCount, sets);
        for (SlangInt s = 0; s < sets; s++)
        {
            put(id, Query::TypeLayoutDescriptorSetSpaceOffset, t->getDescriptorSetSpaceOffset(s), int32_t(s));
            SlangInt ranges = t->getDescriptorSetDescriptorRangeCount(s);
            put(id, Query::TypeLayoutDescriptorSetRangeCount, ranges, int32_t(s));
            for (SlangInt r = 0; r < ranges; r++)
            {
                int32_t a0 = int32_t(s), a1 = int32_t(r);
                put(id,
                    Query::TypeLayoutDescriptorSetRangeIndexOffset,
                    t->getDescriptorSetDescriptorRangeIndexOffset(s, r),
                    a0,
                    a1);
                put(id,
                    Query::TypeLayoutDescriptorSetRangeDescriptorCount,
                    t->getDescriptorSetDescriptorRangeDescriptorCount(s, r),
                    a0,
                    a1);
                put(id,
                    Query::TypeLayoutDescriptorSetRangeType,
                    int64_t(t->getDescriptorSetDescriptorRangeType(s, r)),
                    a0,
                    a1);
            }
        }

        SlangInt subObjects = t->getSubObjectRangeCount();
        put(id, Query::TypeLayoutSubObjectRangeCount, subObjects);
        for (SlangInt i = 0; i < subObjects; i++)
        {
            int32_t a = int32_t(i);
            put(id, Query::TypeLayoutSubObjectRangeBindingRangeIndex, t->getSubObjectRangeBindingRangeIndex(i), a);
            put(id, Query::TypeLayoutSubObjectRangeSpaceOffset, t->getSubObjectRangeSpaceOffset(i), a);
            putNode(id, Query::TypeLayoutSubObjectRangeOffset, NodeKind::VariableLayout, t->getSubObjectRangeOffset(i), a);
        }
    }

    void type(uint32_t id, slang::TypeReflection* t)
    {
        auto kind = t->getKind();
        put(id, Query::TypeKind, int64_t(kind));
        putString(id, Query::TypeName, t->getName());
        putNode(id, Query::TypeElementType, NodeKind::Type, t->getElementType());
        put(id, Query::TypeElementCount, int64_t(t->getElementCount()));
        put(id, Query::TypeSpecializedElementCount, int64_t(t->getElementCount(reinterpret_cast<SlangReflection*>(m_program))));
        put(id, Query::TypeRowCount, t->getRowCount());
        put(id, Query::TypeColumnCount, t->getColumnCount());
        put(id, Query::TypeFieldCount, t->getFieldCount());
        put(id, Query::TypeScalarType, int64_t(t->getScalarType()));
        put(id, Query::TypeResourceShape, int64_t(t->getResourceShape()));
        put(id, Query::TypeResourceAccess, int64_t(t->getResourceAccess()));
        putNode(id, Query::TypeResourceResultType, NodeKind::Type, t->getResourceResultType());
        // Only specialized types answer argument queries; other kinds crash the compiler.
        if (kind == slang::TypeReflection::Kind::Specialized)
        {
            auto raw = reinterpret_cast<SlangReflectionType*>(t);
            SlangInt args = spReflectionType_getSpecializedTypeArgCount(raw);
            put(id, Query::TypeSpecializedTypeArgCount, args);
            for (SlangInt i = 0; i < args; i++)
                putNode(
                    id,
                    Query::TypeSpecializedTypeArgType,
                    NodeKind::Type,
                    spReflectionType_getSpecializedTypeArgType(raw, i),
                    int32_t(i)
                );
        }

    }

    slang::ISession* m_session;
    slang::ProgramLayout* m_program;
    bool m_metal;
    LayoutBlob m_blob;
    std::map<std::pair<NodeKind, void*>, uint32_t> m_ids;
    std::map<std::string, uint32_t> m_strings;
    std::deque<std::pair<NodeKind, void*>> m_work;
};

void printDiagnostics(slang::IBlob* diagnostics)
{
    if (diagnostics && diagnostics->getBufferSize() > 0)
        std::fprintf(stderr, "%s\n", static_cast<const char*>(diagnostics->getBufferPointer()));
}

bool writeFile(const std::string& path, const void* data, size_t size)
{
    std::ofstream out(path, std::ios::binary);
    out.write(static_cast<const char*>(data), std::streamsize(size));
    return bool(out);
}

bool endsWith(const std::string& s, const char* suffix)
{
    size_t n = std::strlen(suffix);
    return s.size() >= n && s.compare(s.size() - n, n, suffix) == 0;
}

SlangStage parseStage(const std::string& name)
{
    static const std::pair<const char*, SlangStage> kStages[] = {
        {"compute", SLANG_STAGE_COMPUTE},
        {"vertex", SLANG_STAGE_VERTEX},
        {"fragment", SLANG_STAGE_FRAGMENT},
        {"pixel", SLANG_STAGE_FRAGMENT},
        {"raygeneration", SLANG_STAGE_RAY_GENERATION},
        {"closesthit", SLANG_STAGE_CLOSEST_HIT},
        {"anyhit", SLANG_STAGE_ANY_HIT},
        {"miss", SLANG_STAGE_MISS},
        {"intersection", SLANG_STAGE_INTERSECTION},
        {"callable", SLANG_STAGE_CALLABLE},
        {"mesh", SLANG_STAGE_MESH},
        {"amplification", SLANG_STAGE_AMPLIFICATION},
    };
    for (const auto& [text, stage] : kStages)
        if (name == text)
            return stage;
    return SLANG_STAGE_NONE;
}

struct EntryRequest
{
    std::string name;
    SlangStage stage = SLANG_STAGE_NONE;
    std::string codePath;
};

} // namespace

int main(int argc, char** argv)
{
    std::string layoutPath;
    std::vector<std::string> modulePaths;
    std::vector<EntryRequest> entries;
    std::vector<const char*> slangArgs;
    // Every `-o` with the entry it follows (-1: none). The last one names the layout.
    std::vector<std::pair<int, std::string>> outputs;
    bool afterEntry = false;
    for (int i = 1; i < argc; i++)
    {
        std::string arg = argv[i];
        auto value = [&]() -> const char* { return i + 1 < argc ? argv[++i] : nullptr; };
        bool entryOption = arg == "-entry" || arg == "-stage";
        if (arg == "-o")
        {
            const char* path = value();
            outputs.emplace_back(afterEntry ? int(entries.size()) - 1 : -1, path ? path : "");
        }
        else if (arg == "-reflection-json")
            value();
        else if (arg == "-entry")
        {
            const char* name = value();
            if (name)
                entries.push_back({name, SLANG_STAGE_NONE, {}});
        }
        else if (arg == "-stage")
        {
            const char* stage = value();
            if (!stage || entries.empty() || (entries.back().stage = parseStage(stage)) == SLANG_STAGE_NONE)
            {
                std::fprintf(stderr, "-stage must follow -entry and name a shader stage\n");
                return 2;
            }
        }
        else if (arg[0] != '-' && endsWith(arg, ".slang"))
            modulePaths.push_back(arg);
        else
            slangArgs.push_back(argv[i]);
        afterEntry = entryOption;
    }
    if (!outputs.empty())
    {
        layoutPath = outputs.back().second;
        outputs.pop_back();
    }
    for (const auto& [entry, path] : outputs)
    {
        if (entry < 0 || !entries[size_t(entry)].codePath.empty())
        {
            std::fprintf(stderr, "only the last -o may stand alone; others must follow an -entry\n");
            return 2;
        }
        entries[size_t(entry)].codePath = path;
    }
    if (modulePaths.empty() || layoutPath.empty())
    {
        std::fprintf(stderr, "usage: dream-slang-layout <module.slang>... [-entry <name> [-stage <stage>]]... "
                             "<slangc options> -target <target> -o <layout.bin>\n");
        return 2;
    }

    ComPtr<slang::IGlobalSession> global;
    if (SLANG_FAILED(slang::createGlobalSession(global.writeRef())))
        return 1;

    slang::SessionDesc sessionDesc = {};
    ComPtr<ISlangUnknown> aux;
    if (SLANG_FAILED(global->parseCommandLineArguments(
            int(slangArgs.size()),
            slangArgs.data(),
            &sessionDesc,
            aux.writeRef()
        )))
    {
        std::fprintf(stderr, "could not parse slangc options\n");
        return 1;
    }
    if (sessionDesc.targetCount != 1)
    {
        std::fprintf(stderr, "exactly one -target is required, got %d\n", int(sessionDesc.targetCount));
        return 1;
    }

    ComPtr<slang::ISession> session;
    if (SLANG_FAILED(global->createSession(sessionDesc, session.writeRef())))
        return 1;

    std::vector<slang::IComponentType*> components;
    std::vector<slang::IModule*> modules;
    ComPtr<slang::IBlob> diagnostics;
    for (const std::string& path : modulePaths)
    {
        diagnostics = nullptr;
        slang::IModule* module = session->loadModule(path.c_str(), diagnostics.writeRef());
        printDiagnostics(diagnostics);
        if (!module)
            return 1;
        modules.push_back(module);
        components.push_back(module);
    }

    std::vector<ComPtr<slang::IEntryPoint>> entryPoints;
    if (entries.empty())
    {
        for (slang::IModule* module : modules)
            for (SlangInt32 i = 0; i < module->getDefinedEntryPointCount(); i++)
            {
                ComPtr<slang::IEntryPoint> entryPoint;
                if (SLANG_SUCCEEDED(module->getDefinedEntryPoint(i, entryPoint.writeRef())))
                    entryPoints.push_back(entryPoint);
            }
    }
    for (const EntryRequest& request : entries)
    {
        ComPtr<slang::IEntryPoint> entryPoint;
        for (slang::IModule* module : modules)
        {
            diagnostics = nullptr;
            SlangResult found = request.stage == SLANG_STAGE_NONE
                                    ? module->findEntryPointByName(request.name.c_str(), entryPoint.writeRef())
                                    : module->findAndCheckEntryPoint(
                                          request.name.c_str(),
                                          request.stage,
                                          entryPoint.writeRef(),
                                          diagnostics.writeRef()
                                      );
            if (SLANG_SUCCEEDED(found) && entryPoint)
                break;
        }
        if (!entryPoint)
        {
            printDiagnostics(diagnostics);
            std::fprintf(stderr, "entry point '%s' not found\n", request.name.c_str());
            return 1;
        }
        entryPoints.push_back(entryPoint);
    }
    if (entryPoints.empty())
    {
        std::fprintf(stderr, "the module defines no entry points\n");
        return 1;
    }
    for (auto& entryPoint : entryPoints)
        components.push_back(entryPoint);

    ComPtr<slang::IComponentType> composite, linked;
    diagnostics = nullptr;
    if (SLANG_FAILED(session->createCompositeComponentType(
            components.data(),
            SlangInt(components.size()),
            composite.writeRef(),
            diagnostics.writeRef()
        )))
    {
        printDiagnostics(diagnostics);
        return 1;
    }
    diagnostics = nullptr;
    if (SLANG_FAILED(composite->link(linked.writeRef(), diagnostics.writeRef())))
    {
        printDiagnostics(diagnostics);
        return 1;
    }

    slang::ProgramLayout* layout = linked->getLayout(0, diagnostics.writeRef());
    if (!layout)
    {
        printDiagnostics(diagnostics);
        return 1;
    }

    SlangCompileTarget format = sessionDesc.targets[0].format;
    bool metal = format == SLANG_METAL || format == SLANG_METAL_LIB || format == SLANG_METAL_LIB_ASM;
    LayoutBlob blob = Recorder(session, layout, metal).run();
    std::vector<uint8_t> bytes = blob.serialize();
    if (!writeFile(layoutPath, bytes.data(), bytes.size()))
    {
        std::fprintf(stderr, "could not write %s\n", layoutPath.c_str());
        return 1;
    }
    for (size_t i = 0; i < entries.size(); i++)
    {
        if (entries[i].codePath.empty())
            continue;
        ComPtr<slang::IBlob> code;
        diagnostics = nullptr;
        if (SLANG_FAILED(linked->getEntryPointCode(SlangInt(i), 0, code.writeRef(), diagnostics.writeRef())))
        {
            printDiagnostics(diagnostics);
            return 1;
        }
        printDiagnostics(diagnostics);
        if (!writeFile(entries[i].codePath, code->getBufferPointer(), code->getBufferSize()))
        {
            std::fprintf(stderr, "could not write %s\n", entries[i].codePath.c_str());
            return 1;
        }
    }
    return 0;
}
