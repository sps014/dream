#pragma once

// Public entry point of the strict ahead-of-time loader: a slang-rhi program built only from
// precompiled target code plus a recorded layout. The returned component plugs into
// `rhi::ShaderProgramDesc::slangGlobalScope`; slang-rhi then reads its layout and code exactly
// as it would from a compiled Slang program, without any Slang compiler in the process.

#include <slang.h>

#include <cstddef>
#include <cstdint>
#include <string>

namespace dream::slang_replay {

struct PrecompiledEntryPoint
{
    /// The entry point's name in the recorded layout.
    const char* name = nullptr;
    const void* code = nullptr;
    size_t codeSize = 0;
};

/// The process-wide global session that stands in for the compiler's. Borrowed; never freed.
slang::IGlobalSession* globalSession();

/// The program exposes exactly `entryPoints`, in the given order, each matched by name to an
/// entry point the layout recorded. A module's layout records all of its entry points, so one
/// layout serves a compute pipeline (one entry) and a render pipeline (vertex + fragment) alike.
SlangResult createPrecompiledProgram(
    const void* layout,
    size_t layoutSize,
    const PrecompiledEntryPoint* entryPoints,
    uint32_t entryPointCount,
    slang::IComponentType** outProgram,
    std::string* outError
);

} // namespace dream::slang_replay
