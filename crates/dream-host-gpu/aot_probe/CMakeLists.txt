cmake_minimum_required(VERSION 3.25)
project(dream_gpu_aot_probe LANGUAGES C CXX)

# Configured by build.py. The pinned Slang release supplies headers for everyone and the real
# compiler library only for the build-time layout recorder; the probe links the replay library
# in its place, so the compiler library is absent from the probe's load commands.
set(DREAM_SLANG_DIR "" CACHE PATH "Extracted pinned Slang release")
set(DREAM_SLANG_RHI_DIR "" CACHE PATH "Patched pinned slang-rhi source")
if(NOT DREAM_SLANG_DIR OR NOT DREAM_SLANG_RHI_DIR)
    message(FATAL_ERROR "configure through build.py (DREAM_SLANG_DIR / DREAM_SLANG_RHI_DIR)")
endif()

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

add_library(dream-slang-headers INTERFACE)
target_include_directories(dream-slang-headers INTERFACE ${DREAM_SLANG_DIR}/include)

set(DREAM_GPU_NATIVE ${CMAKE_CURRENT_SOURCE_DIR}/../native)
add_library(dream-slang-replay STATIC
    ${DREAM_GPU_NATIVE}/replay/layout_blob.cpp
    ${DREAM_GPU_NATIVE}/replay/reflection_api.cpp
    ${DREAM_GPU_NATIVE}/replay/sessions.cpp
)
target_include_directories(dream-slang-replay PUBLIC ${DREAM_GPU_NATIVE})
target_link_libraries(dream-slang-replay PUBLIC dream-slang-headers)
# Slang's headers would otherwise declare the replayed API as dllimport on Windows.
target_compile_definitions(dream-slang-replay PUBLIC SLANG_STATIC)

# slang-rhi links whatever target is named `slang` when built "from the Slang repo".
add_library(slang ALIAS dream-slang-replay)

set(SLANG_RHI_BUILD_FROM_SLANG_REPO ON CACHE BOOL "" FORCE)
set(SLANG_RHI_FETCH_SLANG OFF CACHE BOOL "" FORCE)
set(SLANG_RHI_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(SLANG_RHI_BUILD_TESTS_WITH_GLFW OFF CACHE BOOL "" FORCE)
set(SLANG_RHI_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SLANG_RHI_INSTALL OFF CACHE BOOL "" FORCE)
set(SLANG_RHI_ENABLE_CPU OFF CACHE BOOL "" FORCE)
add_subdirectory(${DREAM_SLANG_RHI_DIR} slang-rhi EXCLUDE_FROM_ALL)

add_executable(aot_probe src/probe.cpp src/bundle.cpp ${DREAM_GPU_NATIVE}/loader_check.cpp)
target_link_libraries(aot_probe PRIVATE slang-rhi dream-slang-replay)

add_library(dream-slang-compiler SHARED IMPORTED)
if(APPLE)
    set_target_properties(dream-slang-compiler PROPERTIES
        IMPORTED_LOCATION ${DREAM_SLANG_DIR}/lib/libslang-compiler.dylib)
elseif(UNIX)
    set_target_properties(dream-slang-compiler PROPERTIES
        IMPORTED_LOCATION ${DREAM_SLANG_DIR}/lib/libslang-compiler.so)
else()
    set_target_properties(dream-slang-compiler PROPERTIES
        IMPORTED_IMPLIB ${DREAM_SLANG_DIR}/lib/slang-compiler.lib
        IMPORTED_LOCATION ${DREAM_SLANG_DIR}/bin/slang-compiler.dll)
endif()
target_link_libraries(dream-slang-compiler INTERFACE dream-slang-headers)

add_executable(dream-slang-layout ${DREAM_GPU_NATIVE}/recorder/recorder.cpp ${DREAM_GPU_NATIVE}/replay/layout_blob.cpp)
target_include_directories(dream-slang-layout PRIVATE ${DREAM_GPU_NATIVE})
target_link_libraries(dream-slang-layout PRIVATE dream-slang-compiler)
set_target_properties(dream-slang-layout PROPERTIES BUILD_RPATH ${DREAM_SLANG_DIR}/lib)
