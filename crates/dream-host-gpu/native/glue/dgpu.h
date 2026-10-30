#pragma once

// The flat C surface dream-host-gpu's Rust code drives slang-rhi through (mirrored in
// src/ffi.rs). Rust owns handles, validation, threading and guest futures; this layer only
// translates already-validated requests into slang-rhi calls. Every function returns a
// DgpuStatus; on failure `dgpu_message()` describes it for the calling thread.
//
// Threading: callers serialize every call that touches a device, except `dgpu_fence_wait`, which
// may run on another thread while the device is used.

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum DgpuStatus
{
    DGPU_OK = 0,
    DGPU_FAILED = 1,
    DGPU_INVALID = 2,
    DGPU_UNSUPPORTED = 3,
    DGPU_OUT_OF_MEMORY = 4,
    DGPU_DEVICE_LOST = 5,
    DGPU_SURFACE_LOST = 6,
    DGPU_TIMEOUT = 7,
};

typedef struct DgpuDevice DgpuDevice;
typedef struct DgpuProgram DgpuProgram;
typedef struct DgpuPipeline DgpuPipeline;
typedef struct DgpuSurface DgpuSurface;
typedef struct DgpuEncoder DgpuEncoder;
/// A slang-rhi buffer, texture or sampler; released with `dgpu_object_release`.
typedef void DgpuObject;

/// Detail of the calling thread's last failure; valid until its next dgpu call.
const char* dgpu_message(void);

/// `backend` uses GpuBackend numbering: 1 Dx12, 2 Dx11, 3 Vulkan, 4 Metal, 5 Cuda, 6 WebGpu.
/// Returns 1 when slang-rhi was built with the backend.
int dgpu_backend_compiled(int backend);

int dgpu_device_create(int backend, DgpuDevice** out);
void dgpu_device_destroy(DgpuDevice* device);
/// GpuFeature bit mask.
uint32_t dgpu_device_features(DgpuDevice* device);
const char* dgpu_device_adapter_name(DgpuDevice* device);
/// Non-zero once the driver reported the device lost; `dgpu_message` then says why.
int dgpu_device_lost(DgpuDevice* device);
/// Blocks until the device's submission fence reaches `value`, or `DGPU_TIMEOUT` after
/// `timeout_ns`. Safe on any thread.
int dgpu_fence_wait(DgpuDevice* device, uint64_t value, uint64_t timeout_ns);

void dgpu_object_release(DgpuObject* object);

/// `usage` is a GpuBufferUsage mask; `readback` makes a CPU-readable staging buffer instead.
int dgpu_buffer_create(DgpuDevice* device, uint64_t size, uint32_t usage, int readback, DgpuObject** out);
/// Copies `rows` rows of `row_bytes` each, `row_pitch` apart, out of a readback buffer whose
/// writes have completed, packing them tightly into `out`.
int dgpu_buffer_read_mapped(
    DgpuDevice* device,
    DgpuObject* buffer,
    void* out,
    uint64_t row_pitch,
    uint64_t row_bytes,
    uint32_t rows
);

typedef struct DgpuTextureDesc
{
    uint32_t width;
    uint32_t height;
    uint32_t layers;
    uint32_t mips;
    /// GpuTextureFormat.
    uint32_t format;
    /// Bit 0 render target, bit 1 storage.
    uint32_t usage;
    uint32_t samples;
} DgpuTextureDesc;

typedef struct DgpuDrawingTarget {
    uint64_t id;
    uint64_t adapter;
    uintptr_t texture;
    uintptr_t fence;
    uint64_t wait_value;
    uint64_t signal_value;
    uint32_t width;
    uint32_t height;
    uint32_t format;
} DgpuDrawingTarget;
typedef int (*DgpuDrawingProducer)(void*, const DgpuDrawingTarget*, char*, size_t);
uint64_t dgpu_drawing_allocations(DgpuDevice*);
int dgpu_drawing_copy(DgpuDevice*, DgpuObject*, DgpuDrawingProducer, void*, char*, size_t, uint64_t*);

int dgpu_texture_create(DgpuDevice* device, const DgpuTextureDesc* desc, DgpuObject** out);
typedef struct DgpuReadbackLayout
{
    /// Bytes the readback buffer needs.
    uint64_t staging_size;
    /// Distance between rows in the readback buffer.
    uint64_t row_pitch;
    /// Tightly packed bytes per row, as the guest sees them.
    uint64_t row_bytes;
    uint32_t rows;
} DgpuReadbackLayout;

/// How one subresource of `mip` lands in a readback buffer.
int dgpu_texture_readback_layout(DgpuObject* texture, uint32_t mip, DgpuReadbackLayout* out);

/// `desc` follows GpuSamplerDesc.encode: min, mag, mip filter, address u, v, w,
/// compare + 1 or 0, max anisotropy.
int dgpu_sampler_create(DgpuDevice* device, const int32_t* desc, DgpuObject** out);
/// `kind` is the GpuArgKind of `resource`.
int dgpu_descriptor_handle(DgpuDevice* device, DgpuObject* resource, int kind, uint64_t* out);

/// Bytes of a `rows` x `columns` cooperative-vector matrix in the device's inferencing-optimal
/// layout. `component` is slang-rhi's CooperativeVectorComponentType (0 = Float16, 1 = Float32).
int dgpu_mlp_optimal_size(DgpuDevice* device, uint32_t rows, uint32_t columns, int component, uint64_t* out);
/// Converts a tightly packed row-major matrix into the inferencing-optimal layout on the CPU.
/// `dst_size` must be the size `dgpu_mlp_optimal_size` reported.
int dgpu_mlp_convert_optimal(
    DgpuDevice* device,
    const void* src,
    uint32_t rows,
    uint32_t columns,
    int component,
    void* dst,
    uint64_t dst_size
);

typedef struct DgpuEntryCode
{
    const char* name;
    const void* code;
    size_t size;
} DgpuEntryCode;

/// A strict-AOT program over `entries` (matched by name to `layout`'s recorded entry points).
int dgpu_program_create(
    DgpuDevice* device,
    const void* layout,
    size_t layout_size,
    const DgpuEntryCode* entries,
    uint32_t entry_count,
    DgpuProgram** out
);
void dgpu_program_release(DgpuProgram* program);

/// One bundle parameter row (docs/internals/18-gpu-shader-bundle.md).
typedef struct DgpuParam
{
    const char* name;
    int32_t kind;
    uint32_t flags;
    uint32_t space;
    uint32_t binding;
    uint32_t offset;
    uint32_t size;
} DgpuParam;

/// The parameters of each program entry point, in program entry order.
typedef struct DgpuStageParams
{
    const DgpuParam* params;
    uint32_t count;
} DgpuStageParams;

int dgpu_compute_pipeline_create(
    DgpuDevice* device,
    DgpuProgram* program,
    const DgpuStageParams* stage,
    DgpuPipeline** out
);

typedef struct DgpuVertexAttribute
{
    uint32_t location;
    /// GpuVertexFormat.
    uint32_t format;
    uint32_t offset;
    uint32_t slot;
} DgpuVertexAttribute;

typedef struct DgpuVertexBufferLayout
{
    uint32_t stride;
    uint32_t per_instance;
} DgpuVertexBufferLayout;

typedef struct DgpuRenderDesc
{
    /// GpuTextureFormat, GpuTopology, GpuCullMode, GpuBlendMode.
    uint32_t color_format;
    uint32_t topology;
    uint32_t cull;
    uint32_t blend;
    /// GpuTextureFormat + 1, or 0 without depth.
    uint32_t depth_format;
    /// GpuCompare.
    uint32_t depth_compare;
    uint32_t depth_write;
    uint32_t samples;
    const DgpuVertexBufferLayout* buffers;
    uint32_t buffer_count;
    const DgpuVertexAttribute* attributes;
    uint32_t attribute_count;
} DgpuRenderDesc;

/// `stages` holds the vertex then the fragment parameters.
int dgpu_render_pipeline_create(
    DgpuDevice* device,
    DgpuProgram* program,
    const DgpuStageParams* stages,
    const DgpuRenderDesc* desc,
    DgpuPipeline** out
);
void dgpu_pipeline_release(DgpuPipeline* pipeline);

/// One hit group of a ray tracing pipeline; absent members are NULL.
typedef struct DgpuHitGroup
{
    const char* closest_hit;
    const char* any_hit;
    const char* intersection;
} DgpuHitGroup;

/// The shader binding table of a ray tracing pipeline, by entry point name. Ray generation
/// shader `i` of the table is the one `dgpu_enc_trace_rays` runs with `i`.
typedef struct DgpuRayTracingDesc
{
    const char* ray_generation;
    const char* const* misses;
    uint32_t miss_count;
    const DgpuHitGroup* hit_groups;
    uint32_t hit_group_count;
    const char* const* callables;
    uint32_t callable_count;
    uint32_t max_payload_size;
    uint32_t max_attribute_size;
    uint32_t max_recursion;
} DgpuRayTracingDesc;

/// `stages` holds one parameter list per program entry point, ray generation first.
int dgpu_ray_tracing_pipeline_create(
    DgpuDevice* device,
    DgpuProgram* program,
    const DgpuStageParams* stages,
    uint32_t stage_count,
    const DgpuRayTracingDesc* desc,
    DgpuPipeline** out
);

/// A bottom-level build input: triangles (kind 0, float3 positions with optional 32-bit
/// indices) or boxes (kind 1, one min/max float3 pair per primitive).
typedef struct DgpuGeometry
{
    int32_t kind;
    DgpuObject* vertices;
    uint32_t vertex_count;
    uint32_t vertex_stride;
    DgpuObject* indices;
    uint32_t index_count;
    int32_t opaque;
} DgpuGeometry;

/// One top-level instance: a 3x4 row-major object-to-world transform plus the
/// D3D12/Vulkan instance fields.
typedef struct DgpuInstance
{
    DgpuObject* bottom_level;
    float transform[12];
    uint32_t id;
    uint32_t mask;
    uint32_t hit_group_offset;
    uint32_t flags;
} DgpuInstance;

/// Records a bottom-level build into `e`. `*scratch` (and for top-level builds `*instances`)
/// must stay alive until the encoder's submission finished.
int dgpu_enc_build_bottom_level(
    DgpuEncoder* e,
    const DgpuGeometry* geometry,
    DgpuObject** out,
    DgpuObject** scratch
);
int dgpu_enc_build_top_level(
    DgpuEncoder* e,
    const DgpuInstance* instances,
    uint32_t count,
    DgpuObject** out,
    DgpuObject** scratch,
    DgpuObject** instance_buffer
);
/// Adds one reference to a buffer, texture, sampler or acceleration structure.
void dgpu_object_retain(DgpuObject* object);

int dgpu_encoder_begin(DgpuDevice* device, DgpuEncoder** out);
/// Drops a recording without submitting it.
void dgpu_encoder_abort(DgpuEncoder* encoder);
/// Submits the recording, signals the device fence and returns the value it will reach. The
/// encoder is consumed whether or not submission succeeds.
int dgpu_encoder_submit(DgpuEncoder* encoder, uint64_t* fence_value);

int dgpu_enc_write_buffer(DgpuEncoder* e, DgpuObject* buffer, uint64_t offset, const void* data, uint64_t size);
int dgpu_enc_write_texture(DgpuEncoder* e, DgpuObject* texture, uint32_t mip, uint32_t layer, const void* data, uint64_t size);
int dgpu_enc_copy_buffer(DgpuEncoder* e, DgpuObject* dst, uint64_t dst_offset, DgpuObject* src, uint64_t src_offset, uint64_t size);
/// Copies one subresource into a readback buffer laid out as `dgpu_texture_readback_layout` says.
int dgpu_enc_read_texture(DgpuEncoder* e, DgpuObject* texture, uint32_t mip, uint32_t layer, DgpuObject* readback);

int dgpu_enc_begin_render_pass(
    DgpuEncoder* e,
    DgpuObject* color,
    int clear_color,
    const float* rgba,
    DgpuObject* depth,
    int clear_depth,
    float depth_value
);
int dgpu_enc_begin_compute_pass(DgpuEncoder* e);
int dgpu_enc_end_pass(DgpuEncoder* e);
int dgpu_enc_set_pipeline(DgpuEncoder* e, DgpuPipeline* pipeline);
/// Binds parameter `index` of the bound pipeline's entry `entry` (0 compute/vertex,
/// 1 fragment). Resources pass `resource`; values pass `data`.
int dgpu_enc_bind(DgpuEncoder* e, uint32_t entry, uint32_t index, DgpuObject* resource, const void* data, uint32_t size);
int dgpu_enc_set_vertex_buffer(DgpuEncoder* e, uint32_t slot, DgpuObject* buffer, uint64_t offset);
/// `format` is GpuIndexFormat.
int dgpu_enc_set_index_buffer(DgpuEncoder* e, DgpuObject* buffer, uint32_t format, uint64_t offset);
int dgpu_enc_set_viewport(DgpuEncoder* e, const float* xywh_min_max);
int dgpu_enc_set_scissor(DgpuEncoder* e, int32_t x, int32_t y, int32_t width, int32_t height);
int dgpu_enc_draw(DgpuEncoder* e, uint32_t vertices, uint32_t instances, uint32_t first_vertex, uint32_t first_instance);
int dgpu_enc_draw_indexed(
    DgpuEncoder* e,
    uint32_t indices,
    uint32_t instances,
    uint32_t first_index,
    int32_t base_vertex,
    uint32_t first_instance
);
int dgpu_enc_dispatch(DgpuEncoder* e, uint32_t x, uint32_t y, uint32_t z);
/// Runs the bound ray tracing pipeline's first ray generation shader over a grid.
int dgpu_enc_trace_rays(DgpuEncoder* e, uint32_t width, uint32_t height, uint32_t depth);

/// Platform numbering follows dream_host_abi::window_surface (1 AppKit NSView*, 2 Win32 HWND,
/// 3 Xlib, 4 Wayland). Must run on the window thread.
typedef struct DgpuWindow
{
    uint32_t platform;
    void* window;
    void* display;
    uint64_t xlib_window;
} DgpuWindow;

int dgpu_surface_create(DgpuDevice* device, const DgpuWindow* window, DgpuSurface** out);
/// Configures for `width` x `height`; returns the chosen GpuTextureFormat in `format`.
int dgpu_surface_configure(DgpuSurface* surface, uint32_t width, uint32_t height, int vsync, uint32_t* format);
int dgpu_surface_acquire(DgpuSurface* surface, DgpuObject** texture);
int dgpu_surface_present(DgpuSurface* surface);
void dgpu_surface_release(DgpuSurface* surface);

/// Loader check: the number of Slang compiler images mapped into the process, and the first
/// one's path in `dgpu_message()`.
int dgpu_loaded_slang_compilers(void);

#ifdef __cplusplus
}
#endif
