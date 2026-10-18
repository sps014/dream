#pragma once

// Recorded answers to the Slang reflection queries slang-rhi makes while it builds shader
// object layouts. The build-time recorder fills this from the real compiler; the replay
// library answers the same queries from it at run time, so the Slang compiler library is
// never loaded by a process that only runs precompiled shaders.

#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

namespace dream::slang_replay {

enum class NodeKind : uint8_t
{
    Program = 0,
    EntryPoint = 1,
    VariableLayout = 2,
    TypeLayout = 3,
    Type = 4,
};

// Query identifiers are part of the serialized format: append only, never renumber.
enum class Query : uint16_t
{
    ProgramGlobalParamsVarLayout = 1,
    ProgramEntryPointCount = 2,
    ProgramEntryPointByIndex = 3,

    EntryPointName = 10,
    EntryPointNameOverride = 11,
    EntryPointStage = 12,
    EntryPointVarLayout = 13,
    EntryPointThreadGroupSize = 14,
    EntryPointParameterCount = 15,
    EntryPointParameterByIndex = 16,

    TypeLayoutType = 20,
    TypeLayoutKind = 21,
    TypeLayoutSize = 22,
    TypeLayoutStride = 23,
    TypeLayoutElementStride = 24,
    TypeLayoutCategoryCount = 25,
    TypeLayoutCategoryByIndex = 26,
    TypeLayoutFieldCount = 27,
    TypeLayoutFieldByIndex = 28,
    TypeLayoutElementTypeLayout = 29,
    TypeLayoutElementVarLayout = 30,
    TypeLayoutContainerVarLayout = 31,
    TypeLayoutBindingRangeCount = 32,
    TypeLayoutBindingRangeType = 33,
    TypeLayoutBindingRangeBindingCount = 34,
    TypeLayoutBindingRangeDescriptorRangeCount = 35,
    TypeLayoutBindingRangeDescriptorSetIndex = 36,
    TypeLayoutBindingRangeFirstDescriptorRangeIndex = 37,
    TypeLayoutBindingRangeLeafTypeLayout = 38,
    TypeLayoutBindingRangeSpecializable = 39,
    TypeLayoutDescriptorSetCount = 40,
    TypeLayoutDescriptorSetSpaceOffset = 41,
    TypeLayoutDescriptorSetRangeCount = 42,
    TypeLayoutDescriptorSetRangeDescriptorCount = 43,
    TypeLayoutDescriptorSetRangeIndexOffset = 44,
    TypeLayoutDescriptorSetRangeType = 45,
    TypeLayoutSubObjectRangeCount = 46,
    TypeLayoutSubObjectRangeBindingRangeIndex = 47,
    TypeLayoutSubObjectRangeOffset = 48,
    TypeLayoutFieldBindingRangeOffset = 49,
    TypeLayoutExplicitCounter = 50,
    TypeLayoutParameterCategory = 52,
    TypeLayoutMatrixLayoutMode = 53,
    TypeLayoutSubObjectRangeSpaceOffset = 55,
    TypeLayoutAlignment = 57,

    TypeKind = 70,
    TypeName = 71,
    TypeElementType = 72,
    TypeResourceResultType = 73,
    TypeResourceShape = 74,
    TypeScalarType = 75,
    TypeSpecializedElementCount = 76,
    TypeSpecializedTypeArgCount = 77,
    TypeSpecializedTypeArgType = 78,
    TypeElementCount = 79,
    TypeResourceAccess = 80,
    TypeRowCount = 81,
    TypeColumnCount = 82,
    TypeFieldCount = 83,
    // Answer to `ISession::getTypeLayout(type, 0, rules)`; argument 0 is the rules value.
    SessionTypeLayout = 85,

    VariableLayoutTypeLayout = 100,
    VariableLayoutOffset = 101,
    VariableLayoutSpace = 102,
    VariableLayoutName = 103,
    VariableLayoutSemanticName = 107,
    VariableLayoutSemanticIndex = 108,
    VariableLayoutStage = 109,
};

enum class ValueKind : uint8_t
{
    Int = 0,
    Node = 1,
    String = 2,
};

struct Value
{
    ValueKind kind = ValueKind::Int;
    int64_t payload = 0;
};

struct Entry
{
    uint32_t node = 0;
    Query query = Query::ProgramGlobalParamsVarLayout;
    int32_t arg0 = 0;
    int32_t arg1 = 0;
    Value value;
};

struct LayoutBlob
{
    // Node 0 is always the program.
    std::vector<NodeKind> nodes;
    std::vector<std::string> strings;
    std::vector<Entry> entries;

    std::vector<uint8_t> serialize() const;
    static bool deserialize(const uint8_t* data, size_t size, LayoutBlob& out, std::string& error);
};

inline constexpr char kLayoutMagic[4] = {'D', 'S', 'L', 'R'};
inline constexpr uint32_t kLayoutVersion = 1;

} // namespace dream::slang_replay
