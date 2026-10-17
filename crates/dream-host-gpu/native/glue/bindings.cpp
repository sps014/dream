// Maps bundle parameter rows (kind, space, binding, offset, size) onto slang-rhi cursor paths.
//
// The bundle records the binding numbers each target's reflection reported. The program layout
// here is the recorded one, so the same numbers appear on its fields: a resource matches the
// field holding it, and a value parameter matches the uniform block Slang packed it into, then
// lands at its offset inside that block. Names break ties between scopes.

#include "glue/internal.h"

#include <cstring>

using namespace rhi;

namespace dream::gpu {

namespace {

constexpr int32_t kValue = 6;

struct Scope
{
    int object;
    slang::TypeLayoutReflection* layout;
};

bool isBlock(slang::TypeLayoutReflection* layout)
{
    auto kind = layout->getKind();
    return kind == slang::TypeReflection::Kind::ConstantBuffer ||
           kind == slang::TypeReflection::Kind::ParameterBlock;
}

// The struct whose fields a ShaderCursor on the object addresses.
slang::TypeLayoutReflection* elementStruct(slang::TypeLayoutReflection* layout)
{
    while (layout && isBlock(layout))
        layout = layout->getElementTypeLayout();
    return layout;
}

bool isSpaceCategory(slang::ParameterCategory category)
{
    return category == slang::ParameterCategory::RegisterSpace ||
           category == slang::ParameterCategory::SubElementRegisterSpace;
}

bool bindingMatches(slang::VariableLayoutReflection* field, const DgpuParam& param)
{
    slang::TypeLayoutReflection* layout = field->getTypeLayout();
    if (!layout)
        return false;
    for (unsigned i = 0; i < layout->getCategoryCount(); i++)
    {
        auto category = layout->getCategoryByIndex(i);
        if (category == slang::ParameterCategory::Uniform || category == slang::ParameterCategory::VaryingInput ||
            category == slang::ParameterCategory::VaryingOutput)
            continue;
        size_t offset = field->getOffset(category);
        size_t space = field->getBindingSpace(category);
        // A parameter block owns a whole space: reflection reports the space as its index.
        if (isSpaceCategory(category) ? (offset == param.space || offset == param.binding)
                                      : (offset == param.binding && space == param.space))
            return true;
    }
    return false;
}

// Whether a field can hold a parameter of GpuArgKind `kind`. Binding numbers alone are not
// unique: Metal numbers buffers, textures and samplers in separate index spaces.
bool kindMatches(slang::TypeLayoutReflection* layout, int32_t kind)
{
    using Kind = slang::TypeReflection::Kind;
    Kind k = layout->getKind();
    if (kind == kValue)
        return isBlock(layout) || k == Kind::Scalar || k == Kind::Vector || k == Kind::Matrix || k == Kind::Struct ||
               k == Kind::Array;
    if (kind == 3)
        return k == Kind::SamplerState;
    if (k != Kind::Resource || !layout->getType())
        return false;
    auto shape = layout->getType()->getResourceShape() & SLANG_RESOURCE_BASE_SHAPE_MASK;
    bool buffer = shape == SLANG_STRUCTURED_BUFFER || shape == SLANG_BYTE_ADDRESS_BUFFER || shape == SLANG_TEXTURE_BUFFER;
    bool texture = shape == SLANG_TEXTURE_1D || shape == SLANG_TEXTURE_2D || shape == SLANG_TEXTURE_3D ||
                   shape == SLANG_TEXTURE_CUBE;
    switch (kind)
    {
    case 1: // Buffer
    case 7: // MlpWeights
        return buffer;
    case 2: // Texture
    case 4: // StorageTexture
        return texture;
    case 5: // AccelerationStructure
        return shape == SLANG_ACCELERATION_STRUCTURE;
    default:
        return false;
    }
}

void bindField(const Scope& scope, slang::VariableLayoutReflection* field, const DgpuParam& param, BindingPath& path)
{
    bool block = isBlock(field->getTypeLayout());
    path.object = scope.object;
    path.field = field->getName();
    path.dereference = block && param.kind == kValue;
    path.uniformOffset = path.dereference ? param.offset : 0;
}

// Candidates in priority order: the parameter's own name with a matching binding, its name
// alone, then a matching binding. Entry scopes come before the globals because reflection
// reports entry-point bindings relative to the entry, where they can repeat global numbers.
bool resolveOne(const std::vector<Scope>& scopes, const DgpuParam& param, BindingPath& path)
{
    path.kind = param.kind;
    path.size = param.size;
    bool value = param.kind == kValue;
    auto fits = [&](slang::VariableLayoutReflection* field)
    { return field && field->getName() && field->getTypeLayout() && kindMatches(field->getTypeLayout(), param.kind); };
    auto named = [&](slang::VariableLayoutReflection* field)
    { return param.name && std::strcmp(field->getName(), param.name) == 0; };
    auto bound = [&](slang::VariableLayoutReflection* field)
    { return isBlock(field->getTypeLayout()) == value && bindingMatches(field, param); };
    for (int pass = 0; pass < 3; pass++)
    {
        for (const Scope& scope : scopes)
        {
            slang::TypeLayoutReflection* layout = scope.layout;
            for (unsigned f = 0; layout && f < layout->getFieldCount(); f++)
            {
                slang::VariableLayoutReflection* field = layout->getFieldByIndex(f);
                if (!fits(field))
                    continue;
                bool match = pass == 0 ? named(field) && bound(field) : pass == 1 ? named(field) : bound(field);
                if (match)
                {
                    bindField(scope, field, param, path);
                    return true;
                }
            }
        }
    }
    // Finally entry-point `uniform` parameters, which Slang packs into the entry's own block.
    if (value)
    {
        for (const Scope& scope : scopes)
        {
            if (scope.layout && scope.layout->getSize(slang::ParameterCategory::Uniform) >= param.offset + param.size)
            {
                path.object = scope.object;
                path.field.clear();
                path.dereference = false;
                path.uniformOffset = param.offset;
                return true;
            }
        }
    }
    return false;
}

BufferDesc placeholderBufferDesc()
{
    BufferDesc buffer;
    buffer.size = 256;
    buffer.usage = BufferUsage::ShaderResource | BufferUsage::UnorderedAccess | BufferUsage::ConstantBuffer;
    buffer.defaultState = ResourceState::UnorderedAccess;
    return buffer;
}

TextureDesc placeholderTextureDesc()
{
    TextureDesc texture;
    texture.size = {1, 1, 1};
    texture.format = Format::RGBA8Unorm;
    texture.usage = TextureUsage::ShaderResource | TextureUsage::UnorderedAccess;
    texture.defaultState = ResourceState::ShaderResource;
    return texture;
}

int placeholders(DgpuDevice* device)
{
    if (device->placeholderBuffer)
        return DGPU_OK;
    SamplerDesc sampler;
    Result result = device->device->createBuffer(placeholderBufferDesc(), nullptr, device->placeholderBuffer.writeRef());
    if (SLANG_SUCCEEDED(result))
        result = device->device->createTexture(placeholderTextureDesc(), nullptr, device->placeholderTexture.writeRef());
    if (SLANG_SUCCEEDED(result))
        result = device->device->createSampler(sampler, device->placeholderSampler.writeRef());
    if (SLANG_FAILED(result))
    {
        device->placeholderBuffer = nullptr;
        return failResult(device, result, "creating placeholder bindings failed");
    }
    return DGPU_OK;
}

/// How many writable stand-ins of each kind the current fill has handed out.
struct Fill
{
    size_t mutableBuffers = 0;
    size_t mutableTextures = 0;
};

IBuffer* mutableBuffer(DgpuDevice* device, Fill& fill)
{
    auto& pool = device->placeholderMutableBuffers;
    if (fill.mutableBuffers == pool.size())
    {
        ComPtr<IBuffer> buffer;
        if (SLANG_FAILED(device->device->createBuffer(placeholderBufferDesc(), nullptr, buffer.writeRef())))
            return nullptr;
        pool.push_back(buffer);
    }
    return pool[fill.mutableBuffers++].get();
}

ITexture* mutableTexture(DgpuDevice* device, Fill& fill)
{
    auto& pool = device->placeholderMutableTextures;
    if (fill.mutableTextures == pool.size())
    {
        ComPtr<ITexture> texture;
        if (SLANG_FAILED(device->device->createTexture(placeholderTextureDesc(), nullptr, texture.writeRef())))
            return nullptr;
        pool.push_back(texture);
    }
    return pool[fill.mutableTextures++].get();
}

int fillObject(DgpuDevice* device, IShaderObject* object, Fill& fill, int depth)
{
    slang::TypeLayoutReflection* layout = object ? object->getElementTypeLayout() : nullptr;
    if (!layout || depth > 8)
        return DGPU_OK;
    for (SlangInt range = 0; range < layout->getBindingRangeCount(); range++)
    {
        SlangInt count = layout->getBindingRangeBindingCount(range);
        for (SlangInt i = 0; i < count; i++)
        {
            ShaderOffset offset;
            offset.bindingRangeIndex = uint32_t(range);
            offset.bindingArrayIndex = uint32_t(i);
            switch (layout->getBindingRangeType(range))
            {
            case slang::BindingType::RawBuffer:
                object->setBinding(offset, Binding(device->placeholderBuffer.get()));
                break;
            case slang::BindingType::MutableRawBuffer:
            {
                IBuffer* buffer = mutableBuffer(device, fill);
                if (!buffer)
                    return fail(DGPU_OUT_OF_MEMORY, "creating a placeholder buffer failed");
                object->setBinding(offset, Binding(buffer));
                break;
            }
            case slang::BindingType::Texture:
                object->setBinding(offset, Binding(device->placeholderTexture.get()));
                break;
            case slang::BindingType::MutableTexture:
            {
                ITexture* texture = mutableTexture(device, fill);
                if (!texture)
                    return fail(DGPU_OUT_OF_MEMORY, "creating a placeholder texture failed");
                object->setBinding(offset, Binding(texture));
                break;
            }
            case slang::BindingType::Sampler:
                object->setBinding(offset, Binding(device->placeholderSampler.get()));
                break;
            case slang::BindingType::ConstantBuffer:
            case slang::BindingType::ParameterBlock:
            {
                ComPtr<IShaderObject> sub;
                if (SLANG_SUCCEEDED(object->getObject(offset, sub.writeRef())))
                {
                    int status = fillObject(device, sub, fill, depth + 1);
                    if (status != DGPU_OK)
                        return status;
                }
                break;
            }
            default:
                break;
            }
        }
    }
    return DGPU_OK;
}

} // namespace

int fillUnboundSlots(DgpuDevice* device, IShaderObject* object)
{
    int status = placeholders(device);
    if (status != DGPU_OK)
        return status;
    Fill fill;
    status = fillObject(device, object, fill, 0);
    for (uint32_t i = 0; status == DGPU_OK && i < object->getEntryPointCount(); i++)
    {
        ComPtr<IShaderObject> entry;
        if (SLANG_SUCCEEDED(object->getEntryPoint(i, entry.writeRef())))
            status = fillObject(device, entry, fill, 0);
    }
    return status;
}

int resolveBindings(
    DgpuProgram* program,
    const DgpuStageParams* stages,
    uint32_t stageCount,
    std::vector<std::vector<BindingPath>>& out
)
{
    slang::ProgramLayout* layout = program->component->getLayout(0, nullptr);
    if (!layout)
        return fail(DGPU_INVALID, "the shader program has no layout");
    if (layout->getEntryPointCount() != stageCount)
        return fail(DGPU_INVALID, "the shader program has the wrong number of entry points");
    slang::TypeLayoutReflection* globals = nullptr;
    if (auto var = layout->getGlobalParamsVarLayout())
        globals = elementStruct(var->getTypeLayout());
    out.assign(stageCount, {});
    for (uint32_t s = 0; s < stageCount; s++)
    {
        std::vector<Scope> scopes;
        slang::EntryPointReflection* entry = layout->getEntryPointByIndex(s);
        if (entry && entry->getVarLayout())
            scopes.push_back({int(s), elementStruct(entry->getVarLayout()->getTypeLayout())});
        scopes.push_back({-1, globals});
        for (uint32_t p = 0; p < stages[s].count; p++)
        {
            const DgpuParam& param = stages[s].params[p];
            BindingPath path;
            if (!resolveOne(scopes, param, path))
                return fail(
                    DGPU_INVALID,
                    std::string("shader parameter '") + (param.name ? param.name : "?") +
                        "' has no binding in the compiled layout"
                );
            out[s].push_back(std::move(path));
        }
    }
    return DGPU_OK;
}

} // namespace dream::gpu
