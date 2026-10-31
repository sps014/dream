cmake_minimum_required(VERSION 3.25)
project(dream_host_gpu_native LANGUAGES C CXX)

# Configured by build.rs. Every input is a pinned, hash-verified local file: the Slang release
# supplies headers only, and the slang-rhi dependencies arrive as file:// URLs so configuring
# downloads nothing. The replay library stands in for the Slang compiler as slang-rhi's `slang`
# target, which keeps the compiler library out of the host's load commands.
set(DREAM_SLANG_INCLUDE_DIR "" CACHE PATH "include/ of the pinned Slang release")
set(DREAM_SLANG_RHI_DIR "" CACHE PATH "Patched pinned slang-rhi source")
if(NOT DREAM_SLANG_INCLUDE_DIR OR NOT DREAM_SLANG_RHI_DIR)
    message(FATAL_ERROR "configure through build.rs (DREAM_SLANG_INCLUDE_DIR / DREAM_SLANG_RHI_DIR)")
endif()

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
if(MSVC)
    # Rust links the static archives into a cdylib with the release C runtime.
    set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreadedDLL")
endif()

add_library(dream-slang-headers INTERFACE)
target_include_directories(dream-slang-headers INTERFACE ${DREAM_SLANG_INCLUDE_DIR})

add_library(dream-slang-replay STATIC
    replay/layout_blob.cpp
    replay/reflection_api.cpp
    replay/sessions.cpp
)
target_include_directories(dream-slang-replay PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(dream-slang-replay PUBLIC dream-slang-headers)
# Slang's headers would otherwise declare the replayed API as dllimport on Windows.
target_compile_definitions(dream-slang-replay PUBLIC SLANG_STATIC)
add_library(slang ALIAS dream-slang-replay)

set(SLANG_RHI_BUILD_FROM_SLANG_REPO ON CACHE BOOL "" FORCE)
set(SLANG_RHI_FETCH_SLANG OFF CACHE BOOL "" FORCE)
# DXC is a shader compiler; strict AOT never needs it.
set(SLANG_RHI_FETCH_DXC OFF CACHE BOOL "" FORCE)
set(SLANG_RHI_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(SLANG_RHI_BUILD_TESTS_WITH_GLFW OFF CACHE BOOL "" FORCE)
set(SLANG_RHI_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SLANG_RHI_INSTALL OFF CACHE BOOL "" FORCE)
set(SLANG_RHI_ENABLE_CPU OFF CACHE BOOL "" FORCE)
# These would fetch unpinned SDKs; OptiX is Milestone 6 work.
set(SLANG_RHI_ENABLE_OPTIX OFF CACHE BOOL "" FORCE)
set(SLANG_RHI_ENABLE_NVAPI OFF CACHE BOOL "" FORCE)
set(SLANG_RHI_ENABLE_AGILITY_SDK OFF CACHE BOOL "" FORCE)
set(SLANG_RHI_ENABLE_AFTERMATH OFF CACHE BOOL "" FORCE)
add_subdirectory(${DREAM_SLANG_RHI_DIR} slang-rhi EXCLUDE_FROM_ALL)

add_library(dream-gpu-glue STATIC
    ../../dream-mir/src/runtime/c/core/gpu_bundle.c
    glue/device.cpp
    glue/resources.cpp
    glue/drawing.cpp
    glue/programs.cpp
    glue/bindings.cpp
    glue/encoder.cpp
    glue/surface.cpp
    glue/raytracing.cpp
    loader_check.cpp
)
target_include_directories(dream-gpu-glue PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_CURRENT_SOURCE_DIR}/../../dream-mir/src/runtime/c/include)
target_link_libraries(dream-gpu-glue PRIVATE slang-rhi dream-slang-replay)

# build.rs links these archives in dependency order.
set(DREAM_GPU_ARCHIVES dream-gpu-glue slang-rhi slang-rhi-resources)
foreach(optional slang-rhi-vma slang-rhi-d3d12ma)
    if(TARGET ${optional})
        list(APPEND DREAM_GPU_ARCHIVES ${optional})
    endif()
endforeach()
list(APPEND DREAM_GPU_ARCHIVES dream-slang-replay)
add_custom_target(dream-gpu-native ALL DEPENDS ${DREAM_GPU_ARCHIVES})
install(TARGETS ${DREAM_GPU_ARCHIVES} ARCHIVE DESTINATION lib)
string(REPLACE ";" "\n" DREAM_GPU_ARCHIVE_LINES "${DREAM_GPU_ARCHIVES}")
file(WRITE ${CMAKE_BINARY_DIR}/archives.txt "${DREAM_GPU_ARCHIVE_LINES}\n")
install(FILES ${CMAKE_BINARY_DIR}/archives.txt DESTINATION lib)
