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

pub fn layout_entry(
    b: &GpuBindingMeta,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    let ty = match b.kind.as_str() {
        "uniform" => wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        "storage" => wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage {
                read_only: !b.read_write,
            },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        "sampler" => wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        "depth_texture" => wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        "texture_cube" => wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::Cube,
            multisampled: false,
        },
        "storage_texture" => wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba8Unorm,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        _ => wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
    };
    wgpu::BindGroupLayoutEntry {
        binding: b.binding,
        visibility,
        ty,
        count: None,
    }
}

/// Dense per-group layouts for a pipeline layout. Index `i` is `@group(i)`; unused indices get an
/// empty layout so the array stays contiguous.
pub fn create_group_layouts(
    device: &wgpu::Device,
    plans: &[BindGroupPlan],
    visibility: wgpu::ShaderStages,
    label: &str,
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
                        .map(|b| layout_entry(b, visibility))
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
