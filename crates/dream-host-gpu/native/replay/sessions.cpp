// Stand-ins for the Slang session objects slang-rhi talks to. Only the calls a precompiled
// program needs succeed; anything that would compile, link or specialize Slang code reports
// SLANG_E_NOT_AVAILABLE, so a missing AOT input fails instead of compiling at run time.

#include "replay/replay.h"
#include "replay/replay_layout.h"

#include <atomic>
#include <cstring>
#include <vector>

namespace dream::slang_replay {

// ReplayLayout

std::shared_ptr<ReplayLayout> ReplayLayout::load(const uint8_t* data, size_t size, std::string& error)
{
    LayoutBlob blob;
    if (!LayoutBlob::deserialize(data, size, blob, error))
        return nullptr;

    auto layout = std::make_shared<ReplayLayout>();
    layout->m_nodes.resize(blob.nodes.size());
    for (size_t i = 0; i < blob.nodes.size(); i++)
    {
        layout->m_nodes[i].kind = blob.nodes[i];
        layout->m_nodes[i].owner = layout.get();
    }
    layout->m_strings = std::move(blob.strings);
    for (const Entry& e : blob.entries)
    {
        layout->m_nodes[e.node].answers[key(e.query, e.arg0, e.arg1)] = e.value;
        if (e.query == Query::TypeName && e.value.kind == ValueKind::String)
            layout->m_typesByName.emplace(layout->m_strings[size_t(e.value.payload)], e.node);
    }
    return layout;
}

bool ReplayLayout::selectEntryPoints(const std::vector<std::string>& names, std::string& error)
{
    ReplayNode* root = program();
    int64_t recorded = integer(root, Query::ProgramEntryPointCount);
    std::vector<Value> selected;
    for (const std::string& name : names)
    {
        const Value* match = nullptr;
        for (int64_t i = 0; i < recorded && !match; i++)
        {
            const Value* entry = find(root, Query::ProgramEntryPointByIndex, i, 0);
            if (!entry || entry->kind != ValueKind::Node)
                continue;
            const char* recordedName = string(&m_nodes[size_t(entry->payload)], Query::EntryPointName);
            if (recordedName && name == recordedName)
                match = entry;
        }
        if (!match)
        {
            error = "the layout records no entry point named '" + name + "'";
            return false;
        }
        selected.push_back(*match);
    }
    for (int64_t i = 0; i < recorded; i++)
        root->answers.erase(key(Query::ProgramEntryPointByIndex, i, 0));
    root->answers[key(Query::ProgramEntryPointCount, 0, 0)] = {ValueKind::Int, int64_t(selected.size())};
    for (size_t i = 0; i < selected.size(); i++)
        root->answers[key(Query::ProgramEntryPointByIndex, int64_t(i), 0)] = selected[i];
    return true;
}

uint64_t ReplayLayout::key(Query query, int64_t arg0, int64_t arg1)
{
    return (uint64_t(query) << 48) | ((uint64_t(arg0) & 0xffffff) << 24) | (uint64_t(arg1) & 0xffffff);
}

const Value* ReplayLayout::find(const ReplayNode* node, Query query, int64_t arg0, int64_t arg1) const
{
    auto it = node->answers.find(key(query, arg0, arg1));
    return it == node->answers.end() ? nullptr : &it->second;
}

int64_t ReplayLayout::integer(const ReplayNode* node, Query query, int64_t arg0, int64_t arg1) const
{
    const Value* v = find(node, query, arg0, arg1);
    return v && v->kind == ValueKind::Int ? v->payload : 0;
}

ReplayNode* ReplayLayout::node(const ReplayNode* node, Query query, int64_t arg0, int64_t arg1)
{
    const Value* v = find(node, query, arg0, arg1);
    return v && v->kind == ValueKind::Node ? &m_nodes[size_t(v->payload)] : nullptr;
}

const char* ReplayLayout::string(const ReplayNode* node, Query query, int64_t arg0, int64_t arg1) const
{
    const Value* v = find(node, query, arg0, arg1);
    return v && v->kind == ValueKind::String ? m_strings[size_t(v->payload)].c_str() : nullptr;
}

ReplayNode* ReplayLayout::typeByName(const char* name) const
{
    auto it = m_typesByName.find(name);
    return it == m_typesByName.end() ? nullptr : const_cast<ReplayNode*>(&m_nodes[it->second]);
}

namespace {

bool sameGuid(const SlangUUID& a, const SlangUUID& b)
{
    return std::memcmp(&a, &b, sizeof(SlangUUID)) == 0;
}

template<typename... Interfaces>
class ComObject : public Interfaces...
{
public:
    virtual ~ComObject() = default;

    uint32_t addRefImpl() { return ++m_refCount; }
    uint32_t releaseImpl()
    {
        uint32_t count = --m_refCount;
        if (count == 0)
            delete this;
        return count;
    }

private:
    std::atomic<uint32_t> m_refCount{1};
};

#define DREAM_COM_UNKNOWN(PRIMARY)                                                                                     \
    SLANG_NO_THROW SlangResult SLANG_MCALL queryInterface(SlangUUID const& uuid, void** outObject) override             \
    {                                                                                                                  \
        if (sameGuid(uuid, ISlangUnknown::getTypeGuid()) || sameGuid(uuid, PRIMARY::getTypeGuid()))                  \
        {                                                                                                              \
            addRef();                                                                                                  \
            *outObject = static_cast<PRIMARY*>(this);                                                                  \
            return SLANG_OK;                                                                                           \
        }                                                                                                              \
        *outObject = nullptr;                                                                                          \
        return SLANG_E_NO_INTERFACE;                                                                                   \
    }                                                                                                                  \
    SLANG_NO_THROW uint32_t SLANG_MCALL addRef() override                                                              \
    {                                                                                                                  \
        return addRefImpl();                                                                                           \
    }                                                                                                                  \
    SLANG_NO_THROW uint32_t SLANG_MCALL release() override                                                             \
    {                                                                                                                  \
        return releaseImpl();                                                                                          \
    }

class Blob final : public ComObject<ISlangBlob>
{
public:
    explicit Blob(std::vector<uint8_t> bytes)
        : m_bytes(std::move(bytes))
    {
    }

    DREAM_COM_UNKNOWN(ISlangBlob)

    SLANG_NO_THROW void const* SLANG_MCALL getBufferPointer() override { return m_bytes.data(); }
    SLANG_NO_THROW size_t SLANG_MCALL getBufferSize() override { return m_bytes.size(); }

private:
    std::vector<uint8_t> m_bytes;
};

void setNotAvailable(ISlangBlob** outDiagnostics, const char* what)
{
    if (!outDiagnostics)
        return;
    std::string message = std::string(what) + " needs the Slang compiler, which strict ahead-of-time loading excludes";
    const char* text = message.c_str();
    *outDiagnostics = new Blob(std::vector<uint8_t>(text, text + message.size() + 1));
}

class GlobalSession final : public slang::IGlobalSession
{
public:
    SLANG_NO_THROW SlangResult SLANG_MCALL queryInterface(SlangUUID const& uuid, void** outObject) override
    {
        if (sameGuid(uuid, ISlangUnknown::getTypeGuid()) || sameGuid(uuid, slang::IGlobalSession::getTypeGuid()))
        {
            *outObject = static_cast<slang::IGlobalSession*>(this);
            return SLANG_OK;
        }
        *outObject = nullptr;
        return SLANG_E_NO_INTERFACE;
    }
    // The global session is a process-lifetime singleton.
    SLANG_NO_THROW uint32_t SLANG_MCALL addRef() override { return 1; }
    SLANG_NO_THROW uint32_t SLANG_MCALL release() override { return 1; }

    SLANG_NO_THROW SlangResult SLANG_MCALL createSession(slang::SessionDesc const&, slang::ISession** outSession) override;
    SLANG_NO_THROW SlangProfileID SLANG_MCALL findProfile(char const*) override { return SLANG_PROFILE_UNKNOWN; }
    SLANG_NO_THROW void SLANG_MCALL setDownstreamCompilerPath(SlangPassThrough, char const*) override {}
    SLANG_NO_THROW void SLANG_MCALL setDownstreamCompilerPrelude(SlangPassThrough, const char*) override {}
    SLANG_NO_THROW void SLANG_MCALL getDownstreamCompilerPrelude(SlangPassThrough, ISlangBlob** outPrelude) override
    {
        *outPrelude = nullptr;
    }
    SLANG_NO_THROW const char* SLANG_MCALL getBuildTagString() override { return "dream-strict-aot"; }
    SLANG_NO_THROW SlangResult SLANG_MCALL setDefaultDownstreamCompiler(SlangSourceLanguage, SlangPassThrough) override
    {
        return SLANG_E_NOT_AVAILABLE;
    }
    SlangPassThrough SLANG_MCALL getDefaultDownstreamCompiler(SlangSourceLanguage) override
    {
        return SLANG_PASS_THROUGH_NONE;
    }
    SLANG_NO_THROW void SLANG_MCALL setLanguagePrelude(SlangSourceLanguage, const char*) override {}
    SLANG_NO_THROW void SLANG_MCALL getLanguagePrelude(SlangSourceLanguage, ISlangBlob** outPrelude) override
    {
        *outPrelude = nullptr;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL createCompileRequest(slang::ICompileRequest** outRequest) override
    {
        *outRequest = nullptr;
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW void SLANG_MCALL addBuiltins(char const*, char const*) override {}
    SLANG_NO_THROW void SLANG_MCALL setSharedLibraryLoader(ISlangSharedLibraryLoader*) override {}
    SLANG_NO_THROW ISlangSharedLibraryLoader* SLANG_MCALL getSharedLibraryLoader() override { return nullptr; }
    SLANG_NO_THROW SlangResult SLANG_MCALL checkCompileTargetSupport(SlangCompileTarget) override
    {
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL checkPassThroughSupport(SlangPassThrough) override
    {
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL compileCoreModule(slang::CompileCoreModuleFlags) override
    {
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL loadCoreModule(const void*, size_t) override { return SLANG_E_NOT_AVAILABLE; }
    SLANG_NO_THROW SlangResult SLANG_MCALL saveCoreModule(SlangArchiveType, ISlangBlob** outBlob) override
    {
        *outBlob = nullptr;
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangCapabilityID SLANG_MCALL findCapability(char const*) override
    {
        return SLANG_CAPABILITY_UNKNOWN;
    }
    SLANG_NO_THROW void SLANG_MCALL
    setDownstreamCompilerForTransition(SlangCompileTarget, SlangCompileTarget, SlangPassThrough) override
    {
    }
    SLANG_NO_THROW SlangPassThrough SLANG_MCALL
    getDownstreamCompilerForTransition(SlangCompileTarget, SlangCompileTarget) override
    {
        return SLANG_PASS_THROUGH_NONE;
    }
    SLANG_NO_THROW void SLANG_MCALL getCompilerElapsedTime(double* outTotalTime, double* outDownstreamTime) override
    {
        *outTotalTime = 0.0;
        *outDownstreamTime = 0.0;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL setSPIRVCoreGrammar(char const*) override { return SLANG_E_NOT_AVAILABLE; }
    SLANG_NO_THROW SlangResult SLANG_MCALL
    parseCommandLineArguments(int, const char* const*, slang::SessionDesc*, ISlangUnknown**) override
    {
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL getSessionDescDigest(slang::SessionDesc*, ISlangBlob** outBlob) override
    {
        *outBlob = nullptr;
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL
    compileBuiltinModule(slang::BuiltinModuleName, slang::CompileCoreModuleFlags) override
    {
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL loadBuiltinModule(slang::BuiltinModuleName, const void*, size_t) override
    {
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL
    saveBuiltinModule(slang::BuiltinModuleName, SlangArchiveType, ISlangBlob** outBlob) override
    {
        *outBlob = nullptr;
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL getDownstreamCompilerPath(SlangPassThrough, ISlangBlob** outPath) override
    {
        *outPath = nullptr;
        return SLANG_E_NOT_AVAILABLE;
    }
};

GlobalSession g_globalSession;

class Session final : public ComObject<slang::ISession>
{
public:
    explicit Session(std::shared_ptr<ReplayLayout> layout)
        : m_layout(std::move(layout))
    {
    }

    DREAM_COM_UNKNOWN(slang::ISession)

    SLANG_NO_THROW slang::IGlobalSession* SLANG_MCALL getGlobalSession() override { return &g_globalSession; }
    SLANG_NO_THROW slang::IModule* SLANG_MCALL loadModule(const char*, slang::IBlob** outDiagnostics) override
    {
        setNotAvailable(outDiagnostics, "loadModule");
        return nullptr;
    }
    SLANG_NO_THROW slang::IModule* SLANG_MCALL
    loadModuleFromSource(const char*, const char*, slang::IBlob*, slang::IBlob** outDiagnostics) override
    {
        setNotAvailable(outDiagnostics, "loadModuleFromSource");
        return nullptr;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL createCompositeComponentType(
        slang::IComponentType* const*,
        SlangInt,
        slang::IComponentType** outComposite,
        ISlangBlob** outDiagnostics
    ) override
    {
        *outComposite = nullptr;
        setNotAvailable(outDiagnostics, "createCompositeComponentType");
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW slang::TypeReflection* SLANG_MCALL
    specializeType(slang::TypeReflection*, slang::SpecializationArg const*, SlangInt, ISlangBlob** outDiagnostics) override
    {
        setNotAvailable(outDiagnostics, "specializeType");
        return nullptr;
    }
    SLANG_NO_THROW slang::TypeLayoutReflection* SLANG_MCALL getTypeLayout(
        slang::TypeReflection* type,
        SlangInt targetIndex,
        slang::LayoutRules rules,
        ISlangBlob** outDiagnostics
    ) override
    {
        ReplayNode* node = asNode(type);
        if (!m_layout || !node || node->owner != m_layout.get() || targetIndex != 0)
        {
            setNotAvailable(outDiagnostics, "getTypeLayout for a type outside the recorded program");
            return nullptr;
        }
        return asReflection<slang::TypeLayoutReflection>(m_layout->node(node, Query::SessionTypeLayout, int64_t(rules)));
    }
    SLANG_NO_THROW slang::TypeReflection* SLANG_MCALL
    getContainerType(slang::TypeReflection*, slang::ContainerType, ISlangBlob** outDiagnostics) override
    {
        setNotAvailable(outDiagnostics, "getContainerType");
        return nullptr;
    }
    SLANG_NO_THROW slang::TypeReflection* SLANG_MCALL getDynamicType() override { return nullptr; }
    SLANG_NO_THROW SlangResult SLANG_MCALL getTypeRTTIMangledName(slang::TypeReflection*, ISlangBlob** outName) override
    {
        *outName = nullptr;
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL
    getTypeConformanceWitnessMangledName(slang::TypeReflection*, slang::TypeReflection*, ISlangBlob** outName) override
    {
        *outName = nullptr;
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL
    getTypeConformanceWitnessSequentialID(slang::TypeReflection*, slang::TypeReflection*, uint32_t*) override
    {
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL createCompileRequest(SlangCompileRequest** outRequest) override
    {
        *outRequest = nullptr;
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL createTypeConformanceComponentType(
        slang::TypeReflection*,
        slang::TypeReflection*,
        slang::ITypeConformance** outConformance,
        SlangInt,
        ISlangBlob** outDiagnostics
    ) override
    {
        *outConformance = nullptr;
        setNotAvailable(outDiagnostics, "createTypeConformanceComponentType");
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW slang::IModule* SLANG_MCALL
    loadModuleFromIRBlob(const char*, const char*, slang::IBlob*, slang::IBlob** outDiagnostics) override
    {
        setNotAvailable(outDiagnostics, "loadModuleFromIRBlob");
        return nullptr;
    }
    SLANG_NO_THROW SlangInt SLANG_MCALL getLoadedModuleCount() override { return 0; }
    SLANG_NO_THROW slang::IModule* SLANG_MCALL getLoadedModule(SlangInt) override { return nullptr; }
    SLANG_NO_THROW bool SLANG_MCALL isBinaryModuleUpToDate(const char*, slang::IBlob*) override { return false; }
    SLANG_NO_THROW slang::IModule* SLANG_MCALL
    loadModuleFromSourceString(const char*, const char*, const char*, slang::IBlob** outDiagnostics) override
    {
        setNotAvailable(outDiagnostics, "loadModuleFromSourceString");
        return nullptr;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL
    getDynamicObjectRTTIBytes(slang::TypeReflection*, slang::TypeReflection*, uint32_t*, uint32_t) override
    {
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL
    loadModuleInfoFromIRBlob(slang::IBlob*, SlangInt&, const char*&, const char*&) override
    {
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL getDeclSourceLocation(slang::DeclReflection*, slang::SourceLocation*) override
    {
        return SLANG_E_NOT_AVAILABLE;
    }

private:
    std::shared_ptr<ReplayLayout> m_layout;
};

SlangResult GlobalSession::createSession(slang::SessionDesc const&, slang::ISession** outSession)
{
    *outSession = new Session(nullptr);
    return SLANG_OK;
}

class PrecompiledProgram final : public ComObject<slang::IComponentType>
{
public:
    PrecompiledProgram(std::shared_ptr<ReplayLayout> layout, std::vector<std::vector<uint8_t>> code)
        : m_layout(std::move(layout))
        , m_code(std::move(code))
    {
        m_session = new Session(m_layout);
        m_layout->session = m_session;
    }
    ~PrecompiledProgram() override
    {
        m_layout->session = nullptr;
        m_session->release();
    }

    DREAM_COM_UNKNOWN(slang::IComponentType)

    SLANG_NO_THROW slang::ISession* SLANG_MCALL getSession() override { return m_session; }
    SLANG_NO_THROW slang::ProgramLayout* SLANG_MCALL getLayout(SlangInt targetIndex, slang::IBlob**) override
    {
        return targetIndex == 0 ? asReflection<slang::ProgramLayout>(m_layout->program()) : nullptr;
    }
    SLANG_NO_THROW SlangInt SLANG_MCALL getSpecializationParamCount() override { return 0; }
    SLANG_NO_THROW SlangResult SLANG_MCALL
    getEntryPointCode(SlangInt entryPointIndex, SlangInt targetIndex, slang::IBlob** outCode, slang::IBlob**) override
    {
        if (targetIndex != 0 || entryPointIndex < 0 || size_t(entryPointIndex) >= m_code.size())
        {
            *outCode = nullptr;
            return SLANG_E_INVALID_ARG;
        }
        *outCode = new Blob(m_code[size_t(entryPointIndex)]);
        return SLANG_OK;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL
    getResultAsFileSystem(SlangInt, SlangInt, ISlangMutableFileSystem** outFileSystem) override
    {
        *outFileSystem = nullptr;
        return SLANG_E_NOT_AVAILABLE;
    }
    // The persistent shader cache keys on this; the code itself is a sound key for code that
    // is never regenerated.
    SLANG_NO_THROW void SLANG_MCALL getEntryPointHash(SlangInt entryPointIndex, SlangInt, slang::IBlob** outHash) override
    {
        if (entryPointIndex < 0 || size_t(entryPointIndex) >= m_code.size())
        {
            *outHash = nullptr;
            return;
        }
        *outHash = new Blob(m_code[size_t(entryPointIndex)]);
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL specialize(
        slang::SpecializationArg const*,
        SlangInt,
        slang::IComponentType** outSpecialized,
        ISlangBlob** outDiagnostics
    ) override
    {
        *outSpecialized = nullptr;
        setNotAvailable(outDiagnostics, "specialize");
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL link(slang::IComponentType** outLinked, ISlangBlob**) override
    {
        addRef();
        *outLinked = this;
        return SLANG_OK;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL
    getEntryPointHostCallable(int, int, ISlangSharedLibrary** outSharedLibrary, slang::IBlob** outDiagnostics) override
    {
        *outSharedLibrary = nullptr;
        setNotAvailable(outDiagnostics, "getEntryPointHostCallable");
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL renameEntryPoint(const char*, slang::IComponentType** outEntryPoint) override
    {
        *outEntryPoint = nullptr;
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL linkWithOptions(
        slang::IComponentType** outLinked,
        uint32_t,
        slang::CompilerOptionEntry const*,
        ISlangBlob** outDiagnostics
    ) override
    {
        *outLinked = nullptr;
        setNotAvailable(outDiagnostics, "linkWithOptions");
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL getTargetCode(SlangInt, slang::IBlob** outCode, slang::IBlob**) override
    {
        *outCode = nullptr;
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL
    getTargetMetadata(SlangInt, slang::IMetadata** outMetadata, slang::IBlob**) override
    {
        *outMetadata = nullptr;
        return SLANG_E_NOT_AVAILABLE;
    }
    SLANG_NO_THROW SlangResult SLANG_MCALL
    getEntryPointMetadata(SlangInt, SlangInt, slang::IMetadata** outMetadata, slang::IBlob**) override
    {
        *outMetadata = nullptr;
        return SLANG_E_NOT_AVAILABLE;
    }

private:
    std::shared_ptr<ReplayLayout> m_layout;
    std::vector<std::vector<uint8_t>> m_code;
    Session* m_session = nullptr;
};

} // namespace

slang::IGlobalSession* globalSession()
{
    return &g_globalSession;
}

SlangResult createPrecompiledProgram(
    const void* layoutData,
    size_t layoutSize,
    const PrecompiledEntryPoint* entryPoints,
    uint32_t entryPointCount,
    slang::IComponentType** outProgram,
    std::string* outError
)
{
    *outProgram = nullptr;
    std::string error;
    std::shared_ptr<ReplayLayout> layout =
        ReplayLayout::load(static_cast<const uint8_t*>(layoutData), layoutSize, error);
    if (!layout)
    {
        if (outError)
            *outError = error;
        return SLANG_E_INVALID_ARG;
    }
    std::vector<std::string> names;
    for (uint32_t i = 0; i < entryPointCount; i++)
        names.emplace_back(entryPoints[i].name ? entryPoints[i].name : "");
    if (!layout->selectEntryPoints(names, error))
    {
        if (outError)
            *outError = error;
        return SLANG_E_INVALID_ARG;
    }
    std::vector<std::vector<uint8_t>> code;
    for (uint32_t i = 0; i < entryPointCount; i++)
    {
        auto bytes = static_cast<const uint8_t*>(entryPoints[i].code);
        code.emplace_back(bytes, bytes + entryPoints[i].codeSize);
    }
    *outProgram = new PrecompiledProgram(std::move(layout), std::move(code));
    return SLANG_OK;
}

} // namespace dream::slang_replay

// slang-rhi calls this when the device description carries no global session.
SLANG_EXTERN_C SlangResult slang_createGlobalSession2(const SlangGlobalSessionDesc*, slang::IGlobalSession** out)
{
    *out = dream::slang_replay::globalSession();
    return SLANG_OK;
}
