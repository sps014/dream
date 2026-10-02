//! Bind-group layout construction shared by the compute and render paths.
//!
//! WGSL binding indices are unique per `@group`, so a shader that declares `@group(1)` needs a
//! second bind group rather than a second entry in group 0's layout. A pipeline layout is a dense
//! array indexed by group number, so gaps (a shader using groups 0 and 2) are filled with empty
//! layouts.

use super::abi::GpuBindingMeta;
use indexmap::{IndexMap, IndexSet};

/// One bind group's worth of bindings, in declaration order.
#[derive(Clone)]
pub struct BindGroupPlan {
    pub group: u32,
    pub bindings: Vec<GpuBindingMeta>,
}

/// Groups `binds` by `@group`, dropping duplicate `(group, binding)` pairs. A vertex and fragment
/// stage that both declare the same slot describe one resource, not two.
pub fn plan_groups(binds: &[GpuBindingMeta]) -> Vec<BindGroupPlan> {
    let mut seen = IndexSet::new();
    let mut by_group: IndexMap<u32, Vec<GpuBindingMeta>> = IndexMap::new();
    for b in binds {
        if !seen.insert((b.group, b.binding)) {
            continue;
        }
        by_group.entry(b.group).or_default().push(b.clone());
    }
    by_group.sort_keys();
    by_group
        .into_iter()
        .map(|(group, bindings)| BindGroupPlan { group, bindings })
        .collect()
}

/// The layout entry a binding needs, taken from the shape the shader declared. The emitter records
/// the view dimension, sample type, and storage format/access alongside `kind`, so nothing here
/// has to guess at `2d`/`rgba8unorm`/write-only defaults.
///
/// `uniform_size` is the declared block size, which the uniform entry needs as its
/// `min_binding_size`: the block is a window into a shared ring reached by dynamic offset, and a
/// window with no declared size would run to the end of the ring.
pub fn layout_entry(
    b: &GpuBindingMeta,
    visibility: wgpu::ShaderStages,
    uniform_size: u32,
) -> wgpu::BindGroupLayoutEntry {
    let ty = match b.kind.as_str() {
        "uniform" => wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: std::num::NonZeroU64::new(u64::from(uniform_size)),
        },
        "storage" => wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage {
                read_only: !b.read_write,
            },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        "sampler" => wgpu::BindingType::Sampler(if b.sample_type == "comparison" {
            wgpu::SamplerBindingType::Comparison
        } else {
            wgpu::SamplerBindingType::Filtering
        }),
        "storage_texture" => wgpu::BindingType::StorageTexture {
            access: storage_access(&b.storage_access),
            format: storage_format(&b.storage_format),
            view_dimension: view_dimension(&b.view_dimension),
        },
        _ => wgpu::BindingType::Texture {
            sample_type: sample_type(&b.sample_type),
            view_dimension: view_dimension(&b.view_dimension),
            multisampled: b.multisampled,
        },
    };
    wgpu::BindGroupLayoutEntry {
        binding: b.binding,
        visibility,
        ty,
        count: None,
    }
}

fn view_dimension(name: &str) -> wgpu::TextureViewDimension {
    match name {
        "1d" => wgpu::TextureViewDimension::D1,
        "2d-array" => wgpu::TextureViewDimension::D2Array,
        "cube" => wgpu::TextureViewDimension::Cube,
        "cube-array" => wgpu::TextureViewDimension::CubeArray,
        "3d" => wgpu::TextureViewDimension::D3,
        _ => wgpu::TextureViewDimension::D2,
    }
}

fn sample_type(name: &str) -> wgpu::TextureSampleType {
    match name {
        "depth" => wgpu::TextureSampleType::Depth,
        "unfilterable-float" => wgpu::TextureSampleType::Float { filterable: false },
        _ => wgpu::TextureSampleType::Float { filterable: true },
    }
}

fn storage_access(name: &str) -> wgpu::StorageTextureAccess {
    match name {
        "read-only" => wgpu::StorageTextureAccess::ReadOnly,
        "read-write" => wgpu::StorageTextureAccess::ReadWrite,
        _ => wgpu::StorageTextureAccess::WriteOnly,
    }
}

fn storage_format(name: &str) -> wgpu::TextureFormat {
    dream_abi::gpu_format::by_name(name)
        .map(super::formats::to_wgpu_unchecked)
        .unwrap_or(wgpu::TextureFormat::Rgba8Unorm)
}

/// Dense per-group layouts for a pipeline layout. Index `i` is `@group(i)`; unused indices get an
/// empty layout so the array stays contiguous.
pub fn create_group_layouts(
    device: &wgpu::Device,
    plans: &[BindGroupPlan],
    visibility: wgpu::ShaderStages,
    label: &str,
    uniform_size: u32,
) -> Vec<wgpu::BindGroupLayout> {
    let Some(max_group) = plans.iter().map(|p| p.group).max() else {
        return Vec::new();
    };
    (0..=max_group)
        .map(|g| {
            let entries: Vec<wgpu::BindGroupLayoutEntry> = plans
                .iter()
                .find(|p| p.group == g)
                .map(|p| {
                    p.bindings
                        .iter()
                        .map(|b| layout_entry(b, visibility, uniform_size))
                        .collect()
                })
                .unwrap_or_default();
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(&format!("{label}-bgl{g}")),
                entries: &entries,
            })
        })
        .collect()
}
