// The Slang reflection C API, answered from recorded layouts. These definitions replace the
// compiler library's exports for every function slang-rhi reaches; the set is checked at
// link time, because the probe never links the compiler library.

#include "replay/replay_layout.h"

using dream::slang_replay::asNode;
using dream::slang_replay::asReflection;
using dream::slang_replay::Query;
using dream::slang_replay::ReplayNode;

namespace {

int64_t ask(void* reflection, Query query, int64_t arg0 = 0, int64_t arg1 = 0)
{
    ReplayNode* n = asNode(reflection);
    return n ? n->owner->integer(n, query, arg0, arg1) : 0;
}

template<typename T>
T* askNode(void* reflection, Query query, int64_t arg0 = 0, int64_t arg1 = 0)
{
    ReplayNode* n = asNode(reflection);
    return n ? asReflection<T>(n->owner->node(n, query, arg0, arg1)) : nullptr;
}

const char* askString(void* reflection, Query query, int64_t arg0 = 0)
{
    ReplayNode* n = asNode(reflection);
    return n ? n->owner->string(n, query, arg0) : nullptr;
}

} // namespace

// Program

SlangReflectionVariableLayout* spReflection_getGlobalParamsVarLayout(SlangReflection* r)
{
    return askNode<SlangReflectionVariableLayout>(r, Query::ProgramGlobalParamsVarLayout);
}

SlangReflectionTypeLayout* spReflection_getGlobalParamsTypeLayout(SlangReflection* r)
{
    return spReflectionVariableLayout_GetTypeLayout(spReflection_getGlobalParamsVarLayout(r));
}

SlangUInt spReflection_getEntryPointCount(SlangReflection* r)
{
    return SlangUInt(ask(r, Query::ProgramEntryPointCount));
}

SlangReflectionEntryPoint* spReflection_getEntryPointByIndex(SlangReflection* r, SlangUInt index)
{
    return askNode<SlangReflectionEntryPoint>(r, Query::ProgramEntryPointByIndex, int64_t(index));
}

SlangReflectionEntryPoint* spReflection_findEntryPointByName(SlangReflection* r, char const* name)
{
    SlangUInt count = spReflection_getEntryPointCount(r);
    for (SlangUInt i = 0; i < count; i++)
    {
        SlangReflectionEntryPoint* entryPoint = spReflection_getEntryPointByIndex(r, i);
        const char* candidate = spReflectionEntryPoint_getName(entryPoint);
        if (candidate && name && std::string_view(candidate) == name)
            return entryPoint;
    }
    return nullptr;
}

SlangReflectionType* spReflection_FindTypeByName(SlangReflection* r, char const* name)
{
    ReplayNode* n = asNode(r);
    return n && name ? asReflection<SlangReflectionType>(n->owner->typeByName(name)) : nullptr;
}

slang::ISession* spReflection_GetSession(SlangReflection* r)
{
    ReplayNode* n = asNode(r);
    return n ? n->owner->session : nullptr;
}

// Entry points

char const* spReflectionEntryPoint_getName(SlangReflectionEntryPoint* e)
{
    return askString(e, Query::EntryPointName);
}

char const* spReflectionEntryPoint_getNameOverride(SlangReflectionEntryPoint* e)
{
    return askString(e, Query::EntryPointNameOverride);
}

SlangStage spReflectionEntryPoint_getStage(SlangReflectionEntryPoint* e)
{
    return SlangStage(ask(e, Query::EntryPointStage));
}

SlangReflectionVariableLayout* spReflectionEntryPoint_getVarLayout(SlangReflectionEntryPoint* e)
{
    return askNode<SlangReflectionVariableLayout>(e, Query::EntryPointVarLayout);
}

void spReflectionEntryPoint_getComputeThreadGroupSize(
    SlangReflectionEntryPoint* e,
    SlangUInt axisCount,
    SlangUInt* outSizeAlongAxis
)
{
    for (SlangUInt axis = 0; axis < axisCount; axis++)
        outSizeAlongAxis[axis] = axis < 3 ? SlangUInt(ask(e, Query::EntryPointThreadGroupSize, int64_t(axis))) : 1;
}

unsigned spReflectionEntryPoint_getParameterCount(SlangReflectionEntryPoint* e)
{
    return unsigned(ask(e, Query::EntryPointParameterCount));
}

SlangReflectionVariableLayout* spReflectionEntryPoint_getParameterByIndex(SlangReflectionEntryPoint* e, unsigned index)
{
    return askNode<SlangReflectionVariableLayout>(e, Query::EntryPointParameterByIndex, index);
}

// Type layouts

SlangReflectionType* spReflectionTypeLayout_GetType(SlangReflectionTypeLayout* t)
{
    return askNode<SlangReflectionType>(t, Query::TypeLayoutType);
}

SlangTypeKind spReflectionTypeLayout_getKind(SlangReflectionTypeLayout* t)
{
    return SlangTypeKind(ask(t, Query::TypeLayoutKind));
}

size_t spReflectionTypeLayout_GetSize(SlangReflectionTypeLayout* t, SlangParameterCategory category)
{
    return size_t(ask(t, Query::TypeLayoutSize, category));
}

size_t spReflectionTypeLayout_GetStride(SlangReflectionTypeLayout* t, SlangParameterCategory category)
{
    return size_t(ask(t, Query::TypeLayoutStride, category));
}

int32_t spReflectionTypeLayout_getAlignment(SlangReflectionTypeLayout* t, SlangParameterCategory category)
{
    return int32_t(ask(t, Query::TypeLayoutAlignment, category));
}

size_t spReflectionTypeLayout_GetElementStride(SlangReflectionTypeLayout* t, SlangParameterCategory category)
{
    return size_t(ask(t, Query::TypeLayoutElementStride, category));
}

unsigned spReflectionTypeLayout_GetCategoryCount(SlangReflectionTypeLayout* t)
{
    return unsigned(ask(t, Query::TypeLayoutCategoryCount));
}

SlangParameterCategory spReflectionTypeLayout_GetCategoryByIndex(SlangReflectionTypeLayout* t, unsigned index)
{
    return SlangParameterCategory(ask(t, Query::TypeLayoutCategoryByIndex, index));
}

SlangParameterCategory spReflectionTypeLayout_GetParameterCategory(SlangReflectionTypeLayout* t)
{
    return SlangParameterCategory(ask(t, Query::TypeLayoutParameterCategory));
}

SlangMatrixLayoutMode spReflectionTypeLayout_GetMatrixLayoutMode(SlangReflectionTypeLayout* t)
{
    return SlangMatrixLayoutMode(ask(t, Query::TypeLayoutMatrixLayoutMode));
}

uint32_t spReflectionTypeLayout_GetFieldCount(SlangReflectionTypeLayout* t)
{
    return uint32_t(ask(t, Query::TypeLayoutFieldCount));
}

SlangReflectionVariableLayout* spReflectionTypeLayout_GetFieldByIndex(SlangReflectionTypeLayout* t, unsigned index)
{
    return askNode<SlangReflectionVariableLayout>(t, Query::TypeLayoutFieldByIndex, index);
}

SlangInt spReflectionTypeLayout_findFieldIndexByName(
    SlangReflectionTypeLayout* t,
    const char* nameBegin,
    const char* nameEnd
)
{
    if (!nameBegin)
        return -1;
    std::string_view name = nameEnd ? std::string_view(nameBegin, size_t(nameEnd - nameBegin))
                                    : std::string_view(nameBegin);
    uint32_t count = spReflectionTypeLayout_GetFieldCount(t);
    for (uint32_t i = 0; i < count; i++)
    {
        const char* fieldName = askString(spReflectionTypeLayout_GetFieldByIndex(t, i), Query::VariableLayoutName);
        if (fieldName && name == fieldName)
            return SlangInt(i);
    }
    return -1;
}

SlangInt spReflectionTypeLayout_getFieldBindingRangeOffset(SlangReflectionTypeLayout* t, SlangInt fieldIndex)
{
    return SlangInt(ask(t, Query::TypeLayoutFieldBindingRangeOffset, fieldIndex));
}

SlangReflectionVariableLayout* spReflectionTypeLayout_GetExplicitCounter(SlangReflectionTypeLayout* t)
{
    return askNode<SlangReflectionVariableLayout>(t, Query::TypeLayoutExplicitCounter);
}

SlangReflectionTypeLayout* spReflectionTypeLayout_GetElementTypeLayout(SlangReflectionTypeLayout* t)
{
    return askNode<SlangReflectionTypeLayout>(t, Query::TypeLayoutElementTypeLayout);
}

SlangReflectionVariableLayout* spReflectionTypeLayout_GetElementVarLayout(SlangReflectionTypeLayout* t)
{
    return askNode<SlangReflectionVariableLayout>(t, Query::TypeLayoutElementVarLayout);
}

SlangReflectionVariableLayout* spReflectionTypeLayout_getContainerVarLayout(SlangReflectionTypeLayout* t)
{
    return askNode<SlangReflectionVariableLayout>(t, Query::TypeLayoutContainerVarLayout);
}

SlangInt spReflectionTypeLayout_getBindingRangeCount(SlangReflectionTypeLayout* t)
{
    return SlangInt(ask(t, Query::TypeLayoutBindingRangeCount));
}

SlangBindingType spReflectionTypeLayout_getBindingRangeType(SlangReflectionTypeLayout* t, SlangInt index)
{
    return SlangBindingType(ask(t, Query::TypeLayoutBindingRangeType, index));
}

SlangInt spReflectionTypeLayout_isBindingRangeSpecializable(SlangReflectionTypeLayout* t, SlangInt index)
{
    return SlangInt(ask(t, Query::TypeLayoutBindingRangeSpecializable, index));
}

SlangInt spReflectionTypeLayout_getBindingRangeBindingCount(SlangReflectionTypeLayout* t, SlangInt index)
{
    return SlangInt(ask(t, Query::TypeLayoutBindingRangeBindingCount, index));
}

SlangReflectionTypeLayout* spReflectionTypeLayout_getBindingRangeLeafTypeLayout(
    SlangReflectionTypeLayout* t,
    SlangInt index
)
{
    return askNode<SlangReflectionTypeLayout>(t, Query::TypeLayoutBindingRangeLeafTypeLayout, index);
}

SlangInt spReflectionTypeLayout_getBindingRangeDescriptorSetIndex(SlangReflectionTypeLayout* t, SlangInt index)
{
    return SlangInt(ask(t, Query::TypeLayoutBindingRangeDescriptorSetIndex, index));
}

SlangInt spReflectionTypeLayout_getBindingRangeFirstDescriptorRangeIndex(SlangReflectionTypeLayout* t, SlangInt index)
{
    return SlangInt(ask(t, Query::TypeLayoutBindingRangeFirstDescriptorRangeIndex, index));
}

SlangInt spReflectionTypeLayout_getBindingRangeDescriptorRangeCount(SlangReflectionTypeLayout* t, SlangInt index)
{
    return SlangInt(ask(t, Query::TypeLayoutBindingRangeDescriptorRangeCount, index));
}

SlangInt spReflectionTypeLayout_getDescriptorSetCount(SlangReflectionTypeLayout* t)
{
    return SlangInt(ask(t, Query::TypeLayoutDescriptorSetCount));
}

SlangInt spReflectionTypeLayout_getDescriptorSetSpaceOffset(SlangReflectionTypeLayout* t, SlangInt setIndex)
{
    return SlangInt(ask(t, Query::TypeLayoutDescriptorSetSpaceOffset, setIndex));
}

SlangInt spReflectionTypeLayout_getDescriptorSetDescriptorRangeCount(SlangReflectionTypeLayout* t, SlangInt setIndex)
{
    return SlangInt(ask(t, Query::TypeLayoutDescriptorSetRangeCount, setIndex));
}

SlangInt spReflectionTypeLayout_getDescriptorSetDescriptorRangeIndexOffset(
    SlangReflectionTypeLayout* t,
    SlangInt setIndex,
    SlangInt rangeIndex
)
{
    return SlangInt(ask(t, Query::TypeLayoutDescriptorSetRangeIndexOffset, setIndex, rangeIndex));
}

SlangInt spReflectionTypeLayout_getDescriptorSetDescriptorRangeDescriptorCount(
    SlangReflectionTypeLayout* t,
    SlangInt setIndex,
    SlangInt rangeIndex
)
{
    return SlangInt(ask(t, Query::TypeLayoutDescriptorSetRangeDescriptorCount, setIndex, rangeIndex));
}

SlangBindingType spReflectionTypeLayout_getDescriptorSetDescriptorRangeType(
    SlangReflectionTypeLayout* t,
    SlangInt setIndex,
    SlangInt rangeIndex
)
{
    return SlangBindingType(ask(t, Query::TypeLayoutDescriptorSetRangeType, setIndex, rangeIndex));
}

SlangInt spReflectionTypeLayout_getSubObjectRangeCount(SlangReflectionTypeLayout* t)
{
    return SlangInt(ask(t, Query::TypeLayoutSubObjectRangeCount));
}

SlangInt spReflectionTypeLayout_getSubObjectRangeBindingRangeIndex(SlangReflectionTypeLayout* t, SlangInt index)
{
    return SlangInt(ask(t, Query::TypeLayoutSubObjectRangeBindingRangeIndex, index));
}

SlangInt spReflectionTypeLayout_getSubObjectRangeSpaceOffset(SlangReflectionTypeLayout* t, SlangInt index)
{
    return SlangInt(ask(t, Query::TypeLayoutSubObjectRangeSpaceOffset, index));
}

SlangReflectionVariableLayout* spReflectionTypeLayout_getSubObjectRangeOffset(
    SlangReflectionTypeLayout* t,
    SlangInt index
)
{
    return askNode<SlangReflectionVariableLayout>(t, Query::TypeLayoutSubObjectRangeOffset, index);
}

// Types

SlangTypeKind spReflectionType_GetKind(SlangReflectionType* t)
{
    return SlangTypeKind(ask(t, Query::TypeKind));
}

char const* spReflectionType_GetName(SlangReflectionType* t)
{
    return askString(t, Query::TypeName);
}

SlangReflectionType* spReflectionType_GetElementType(SlangReflectionType* t)
{
    return askNode<SlangReflectionType>(t, Query::TypeElementType);
}

size_t spReflectionType_GetElementCount(SlangReflectionType* t)
{
    return size_t(ask(t, Query::TypeElementCount));
}

size_t spReflectionType_GetSpecializedElementCount(SlangReflectionType* t, SlangReflection*)
{
    return size_t(ask(t, Query::TypeSpecializedElementCount));
}

unsigned int spReflectionType_GetRowCount(SlangReflectionType* t)
{
    return unsigned(ask(t, Query::TypeRowCount));
}

unsigned int spReflectionType_GetColumnCount(SlangReflectionType* t)
{
    return unsigned(ask(t, Query::TypeColumnCount));
}

unsigned int spReflectionType_GetFieldCount(SlangReflectionType* t)
{
    return unsigned(ask(t, Query::TypeFieldCount));
}

SlangScalarType spReflectionType_GetScalarType(SlangReflectionType* t)
{
    return SlangScalarType(ask(t, Query::TypeScalarType));
}

SlangResourceShape spReflectionType_GetResourceShape(SlangReflectionType* t)
{
    return SlangResourceShape(ask(t, Query::TypeResourceShape));
}

SlangResourceAccess spReflectionType_GetResourceAccess(SlangReflectionType* t)
{
    return SlangResourceAccess(ask(t, Query::TypeResourceAccess));
}

SlangReflectionType* spReflectionType_GetResourceResultType(SlangReflectionType* t)
{
    return askNode<SlangReflectionType>(t, Query::TypeResourceResultType);
}

SlangInt spReflectionType_getSpecializedTypeArgCount(SlangReflectionType* t)
{
    return SlangInt(ask(t, Query::TypeSpecializedTypeArgCount));
}

SlangReflectionType* spReflectionType_getSpecializedTypeArgType(SlangReflectionType* t, SlangInt index)
{
    return askNode<SlangReflectionType>(t, Query::TypeSpecializedTypeArgType, index);
}

// Variable layouts

SlangReflectionTypeLayout* spReflectionVariableLayout_GetTypeLayout(SlangReflectionVariableLayout* v)
{
    return askNode<SlangReflectionTypeLayout>(v, Query::VariableLayoutTypeLayout);
}

size_t spReflectionVariableLayout_GetOffset(SlangReflectionVariableLayout* v, SlangParameterCategory category)
{
    return size_t(ask(v, Query::VariableLayoutOffset, category));
}

size_t spReflectionVariableLayout_GetSpace(SlangReflectionVariableLayout* v, SlangParameterCategory category)
{
    return size_t(ask(v, Query::VariableLayoutSpace, category));
}

char const* spReflectionVariableLayout_GetSemanticName(SlangReflectionVariableLayout* v)
{
    return askString(v, Query::VariableLayoutSemanticName);
}

size_t spReflectionVariableLayout_GetSemanticIndex(SlangReflectionVariableLayout* v)
{
    return size_t(ask(v, Query::VariableLayoutSemanticIndex));
}

SlangStage spReflectionVariableLayout_getStage(SlangReflectionVariableLayout* v)
{
    return SlangStage(ask(v, Query::VariableLayoutStage));
}

// A variable layout doubles as its variable: the only variable query slang-rhi needs is the name.
SlangReflectionVariable* spReflectionVariableLayout_GetVariable(SlangReflectionVariableLayout* v)
{
    return reinterpret_cast<SlangReflectionVariable*>(v);
}

char const* spReflectionVariable_GetName(SlangReflectionVariable* v)
{
    return askString(v, Query::VariableLayoutName);
}

// Generated shaders carry no user attributes (the D3D12 backend only asks for `[root]`), and
// D3D12 treats a missing leaf variable as "no attributes".
SlangReflectionVariable* spReflectionTypeLayout_getBindingRangeLeafVariable(SlangReflectionTypeLayout*, SlangInt)
{
    return nullptr;
}

SlangReflectionUserAttribute* spReflectionVariable_FindUserAttributeByName(
    SlangReflectionVariable*,
    SlangSession*,
    char const*
)
{
    return nullptr;
}
