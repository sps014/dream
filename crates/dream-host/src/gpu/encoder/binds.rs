//! Realizing a pipeline's `@group`s into `wgpu::BindGroup`s for one draw.

use super::super::abi::GpuBindingMeta;
use super::super::state::{GpuState, RenderBgKey};
use indexmap::IndexMap;

/// Which owned resource list a bind group entry reads from. `wgpu::BindGroupEntry` borrows its
/// resource, so resources are collected first and referenced by index.
enum Slot {
    Uniform,
    Storage(usize),
    Sampler(usize),
    View(usize),
}

/// Writes one draw's uniform block into the frame's ring and returns its dynamic offset.
///
/// `None` when the pipeline declares no uniform block. The one-draw helpers in `GpuRenderPass`
/// record `set_uniforms` unconditionally, so an empty blob against a shader that takes no uniforms
/// is ordinary rather than a mistake — but a caller that packed real bytes meant them to land
/// somewhere.
pub fn push_uniforms(
    st: &mut GpuState,
    queue: &wgpu::Queue,
    pipeline_id: i32,
    uniforms: &[u8],
) -> Result<Option<u32>, String> {
    let size = st
        .render_pipes
        .get(&pipeline_id)
        .map(|rp| rp.uniform_size)
        .ok_or_else(|| format!("unknown pipeline {pipeline_id}"))?;
    if size == 0 {
        if !uniforms.is_empty() {
            return Err(format!(
                "packed {} bytes of uniforms but pipeline {pipeline_id} declares no uniform block",
                uniforms.len()
            ));
        }
        return Ok(None);
    }
    st.uniform_ring.push(queue, uniforms, size).map(Some)
}

/// Resource ids supplied inline by a `set_bind_list`, spread positionally across every group the
/// pipeline declares.
#[derive(Default)]
pub struct BindList {
    pub buffers: Vec<i32>,
    pub textures: Vec<i32>,
    pub samplers: Vec<i32>,
    buf_at: usize,
    tex_at: usize,
    samp_at: usize,
}

impl BindList {
    pub fn new(buffers: Vec<i32>, textures: Vec<i32>, samplers: Vec<i32>) -> Self {
        Self {
            buffers,
            textures,
            samplers,
            ..Default::default()
        }
    }

    fn take(&mut self, want: (usize, usize, usize)) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
        let cut = |src: &[i32], at: &mut usize, n: usize| {
            let end = (*at + n).min(src.len());
            let out = src[(*at).min(end)..end].to_vec();
            *at = end;
            out
        };
        (
            cut(&self.buffers, &mut self.buf_at, want.0),
            cut(&self.textures, &mut self.tex_at, want.1),
            cut(&self.samplers, &mut self.samp_at, want.2),
        )
    }
}

/// Number of buffer / texture / sampler ids a group consumes.
fn arity(bindings: &[GpuBindingMeta]) -> (usize, usize, usize) {
    let mut want = (0usize, 0usize, 0usize);
    for b in bindings {
        match b.kind.as_str() {
            "uniform" => {}
            "storage" => want.0 += 1,
            "sampler" => want.2 += 1,
            _ => want.1 += 1,
        }
    }
    want
}

/// Builds (or reuses) one bind group per `@group` the pipeline declares.
///
/// `explicit` maps a group index to a `GpuBindGroup` id. Groups without one draw from `list`, in
/// group order, consuming ids positionally per kind — the same order `GpuBindList` appends them.
///
/// Each group comes back with the dynamic offsets `set_bind_group` needs: one entry for its
/// uniform binding if it has one, empty otherwise.
pub fn resolve(
    st: &mut GpuState,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline_id: i32,
    explicit: &IndexMap<u32, i32>,
    list: Option<&BindList>,
    uniform_offset: Option<u32>,
) -> Result<Vec<(u32, wgpu::BindGroup, Vec<u32>)>, String> {
    let groups: Vec<(u32, Vec<GpuBindingMeta>)> = st
        .render_pipes
        .get(&pipeline_id)
        .map(|rp| {
            rp.groups
                .iter()
                .map(|g| (g.group, g.bindings.clone()))
                .collect()
        })
        .unwrap_or_default();

    let mut pending = BindList::new(
        list.map(|l| l.buffers.clone()).unwrap_or_default(),
        list.map(|l| l.textures.clone()).unwrap_or_default(),
        list.map(|l| l.samplers.clone()).unwrap_or_default(),
    );

    let mut out = Vec::new();
    for (group, bindings) in &groups {
        let bind_group_id = explicit.get(group).copied().unwrap_or(-1);
        let has_uniform = bindings.iter().any(|b| b.kind == "uniform");
        let offsets = if has_uniform {
            let offset = uniform_offset.ok_or_else(|| {
                format!("@group({group}) needs a uniform block; call set_uniforms before the draw")
            })?;
            vec![offset]
        } else {
            Vec::new()
        };
        let ids = if bind_group_id >= 0 {
            // A pinned handle brings its own resources; it must not consume from the list, or the
            // groups after it would shift.
            None
        } else {
            Some(pending.take(arity(bindings)))
        };
        let key = RenderBgKey {
            pipeline_id,
            group: *group,
            bind_group_id,
        };
        // Only explicit `GpuBindGroup` handles are cached: their resource set is pinned for the
        // handle's lifetime, whereas a bind list's resources are whatever the caller passed this
        // frame.
        let cacheable = bind_group_id >= 0;
        if cacheable {
            if let Some(bg) = st.render_bg_cache.get(&key) {
                out.push((*group, bg.clone(), offsets));
                continue;
            }
        }
        let bg = build(
            st,
            device,
            queue,
            GroupRequest {
                pipeline_id,
                group: *group,
                bindings,
                bind_group_id,
                ids,
            },
        )?;
        if cacheable {
            st.render_bg_cache.insert(key, bg.clone());
        }
        out.push((*group, bg, offsets));
    }
    Ok(out)
}

/// One group of one pipeline, plus where its resources come from.
struct GroupRequest<'a> {
    pipeline_id: i32,
    group: u32,
    bindings: &'a [GpuBindingMeta],
    bind_group_id: i32,
    /// `Some` when the ids were sliced out of a bind list, `None` when they come from a pinned
    /// `GpuBindGroup` handle.
    ids: Option<(Vec<i32>, Vec<i32>, Vec<i32>)>,
}

fn build(
    st: &mut GpuState,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    req: GroupRequest<'_>,
) -> Result<wgpu::BindGroup, String> {
    let GroupRequest {
        pipeline_id,
        group,
        bindings,
        bind_group_id,
        ids,
    } = req;
    let (buffer_ids, texture_ids, sampler_ids) = match ids {
        Some(triple) => triple,
        None => {
            let e = st
                .bind_groups
                .get(&bind_group_id)
                .ok_or_else(|| format!("unknown bind group {bind_group_id}"))?;
            if e.pipeline_id != pipeline_id || e.group != group {
                return Err(format!(
                    "bind group {bind_group_id} was built for pipeline {} @group({}), not pipeline {pipeline_id} @group({group})",
                    e.pipeline_id, e.group
                ));
            }
            (
                e.buffer_ids.clone(),
                e.texture_ids.clone(),
                e.sampler_ids.clone(),
            )
        }
    };

    let mut storage_bufs: Vec<wgpu::Buffer> = Vec::new();
    let mut samplers: Vec<wgpu::Sampler> = Vec::new();
    let mut views: Vec<wgpu::TextureView> = Vec::new();
    let mut slots: Vec<(u32, Slot)> = Vec::new();
    let mut buf_idx = 0usize;
    let mut tex_idx = 0usize;
    let mut samp_idx = 0usize;

    for b in bindings {
        match b.kind.as_str() {
            "uniform" => slots.push((b.binding, Slot::Uniform)),
            "storage" => {
                let id = *buffer_ids.get(buf_idx).unwrap_or(&-1);
                buf_idx += 1;
                let entry = st.buffers.get_mut(&id).ok_or_else(|| {
                    format!("missing buffer id {id} for @group({group}) @binding({})", b.binding)
                })?;
                super::super::buffers::ensure_gpu_buffer(
                    device,
                    queue,
                    entry,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                )?;
                let g = st
                    .buffers
                    .get(&id)
                    .and_then(|b| b.gpu.clone())
                    .ok_or_else(|| format!("buffer {id} not on GPU"))?;
                slots.push((b.binding, Slot::Storage(storage_bufs.len())));
                storage_bufs.push(g);
            }
            "sampler" => {
                let id = *sampler_ids.get(samp_idx).unwrap_or(&-1);
                samp_idx += 1;
                super::super::compute::ensure_sampler(st, device, id)?;
                let g = st
                    .samplers
                    .get(&id)
                    .and_then(|s| s.gpu.clone())
                    .ok_or_else(|| format!("sampler {id} not on GPU"))?;
                slots.push((b.binding, Slot::Sampler(samplers.len())));
                samplers.push(g);
            }
            "texture" | "storage_texture" => {
                let id = *texture_ids.get(tex_idx).unwrap_or(&-1);
                tex_idx += 1;
                super::super::compute::ensure_texture(
                    st,
                    device,
                    queue,
                    id,
                    b.kind == "storage_texture",
                )?;
                let view = super::super::compute::texture_view(st, id)?;
                slots.push((b.binding, Slot::View(views.len())));
                views.push(view);
            }
            other => return Err(format!("unsupported render binding kind '{other}'")),
        }
    }

    let rp = st
        .render_pipes
        .get(&pipeline_id)
        .ok_or_else(|| format!("unknown pipeline {pipeline_id}"))?;
    let bgl = rp
        .bgls
        .get(group as usize)
        .ok_or_else(|| format!("pipeline {pipeline_id} has no layout for @group({group})"))?;
    let uniform_size = rp.uniform_size;
    // The window starts at 0 and the draw's dynamic offset moves it, so one bind group serves
    // every draw regardless of which slice of the ring that draw wrote.
    let ring = st.uniform_ring.buffer();
    if ring.is_none() && slots.iter().any(|(_, s)| matches!(s, Slot::Uniform)) {
        return Err(format!(
            "@group({group}) needs a uniform block; call set_uniforms before the draw"
        ));
    }
    let entries: Vec<wgpu::BindGroupEntry<'_>> = slots
        .iter()
        .filter_map(|(binding, slot)| {
            Some(wgpu::BindGroupEntry {
            binding: *binding,
            resource: match slot {
                Slot::Uniform => wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: ring?,
                    offset: 0,
                    size: std::num::NonZeroU64::new(u64::from(uniform_size)),
                }),
                Slot::Storage(i) => storage_bufs[*i].as_entire_binding(),
                Slot::Sampler(i) => wgpu::BindingResource::Sampler(&samplers[*i]),
                Slot::View(i) => wgpu::BindingResource::TextureView(&views[*i]),
            },
            })
        })
        .collect();
    Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("dream-draw-bg"),
        layout: bgl,
        entries: &entries,
    }))
}
