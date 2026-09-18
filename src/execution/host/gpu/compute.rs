//! Compute pipeline cache + dispatch.
//! Argument counts mirror the Dream `@js` host ABI (intentional).

#![allow(clippy::too_many_arguments)]

use super::buffers::ensure_gpu_buffer;
use super::error::{classify_err, drain_uncaptured};
use super::formats;
use super::state::{lock_state, ComputePassEntry, ComputePipe, PassOp};
use super::textures::texture_usage;
use indexmap::IndexMap;

/// `(binding, index into the matching owned resource vec, kind tag)`, where the kind tag is
/// `0` uniform, `1` storage buffer, `2` sampler, `3` texture view. The indirection exists because
/// `wgpu::BindGroupEntry` borrows its resource, so every resource must outlive the entry list.
type PlannedEntry = (u32, usize, u8);

pub fn dispatch(
    kernel: &str,
    buffer_ids: &[i32],
    texture_ids: &[i32],
    sampler_ids: &[i32],
    ex: i32,
    ey: i32,
    ez: i32,
    uniforms: &[u8],
) -> i32 {
    match run(
        kernel,
        buffer_ids,
        texture_ids,
        sampler_ids,
        ex,
        ey,
        ez,
        uniforms,
        None,
    ) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Dream gpuDispatch: {e}");
            lock_state().set_last_error(e.clone());
            classify_err(&e)
        }
    }
}

pub fn dispatch_indirect(
    kernel: &str,
    buffer_ids: &[i32],
    texture_ids: &[i32],
    sampler_ids: &[i32],
    indirect_id: i32,
    offset: i32,
) -> i32 {
    match run(
        kernel,
        buffer_ids,
        texture_ids,
        sampler_ids,
        1,
        1,
        1,
        &[],
        Some((indirect_id, offset)),
    ) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Dream gpuDispatchIndirect: {e}");
            lock_state().set_last_error(e.clone());
            classify_err(&e)
        }
    }
}

fn run(
    kernel: &str,
    buffer_ids: &[i32],
    texture_ids: &[i32],
    sampler_ids: &[i32],
    ex: i32,
    ey: i32,
    ez: i32,
    uniforms: &[u8],
    indirect: Option<(i32, i32)>,
) -> Result<(), String> {
    ensure_pipeline(kernel)?;
    let mut st = lock_state();
    if !st.ready {
        return Err("GPU not initialized".into());
    }
    let device = st.device.as_ref().unwrap().clone();
    let queue = st.queue.as_ref().unwrap().clone();
    begin_uniform_frame(&mut st, &device, [kernel.to_string()]);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("dream-dispatch"),
    });
    encode_op(
        &mut st,
        &device,
        &queue,
        Some(&mut encoder),
        None,
        kernel,
        buffer_ids,
        texture_ids,
        sampler_ids,
        ex,
        ey,
        ez,
        uniforms,
        indirect,
    )?;
    queue.submit(Some(encoder.finish()));
    let _ = device.poll(wgpu::Maintain::Poll);
    if let Some(e) = drain_uncaptured() {
        st.set_last_error(e.clone());
        return Err(e);
    }
    Ok(())
}

fn clear_compute_bg_caches(st: &mut super::state::GpuState) {
    for pipe in st.compute_pipes.values_mut() {
        pipe.bg_cache.clear();
    }
}

/// Reserves ring space for every dispatch about to be encoded.
///
/// The whole batch is reserved before the first bind group is built: a group holds the buffer it
/// was built from, so a ring that grew partway through would strand the groups already made.
fn begin_uniform_frame(
    st: &mut super::state::GpuState,
    device: &wgpu::Device,
    kernels: impl IntoIterator<Item = String>,
) {
    let bytes: u64 = kernels
        .into_iter()
        .filter_map(|k| st.compute_pipes.get(&k).map(|p| p.uniform_size))
        .filter(|size| *size > 0)
        .map(|size| super::uniform_ring::UniformRing::stride_for(size, device))
        .sum();
    if st.uniform_ring.begin_frame(device, bytes) {
        clear_compute_bg_caches(st);
    }
}

fn encode_op(
    st: &mut super::state::GpuState,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: Option<&mut wgpu::CommandEncoder>,
    shared_pass: Option<&mut wgpu::ComputePass<'_>>,
    kernel: &str,
    buffer_ids: &[i32],
    texture_ids: &[i32],
    sampler_ids: &[i32],
    ex: i32,
    ey: i32,
    ez: i32,
    uniforms: &[u8],
    indirect: Option<(i32, i32)>,
) -> Result<(), String> {
    let meta = match st
        .abi
        .as_ref()
        .and_then(|a| a.kernels.iter().find(|k| k.name == kernel))
        .cloned()
    {
        Some(meta) => meta,
        None => {
            st.warn_if_gpu_abi_missing();
            return Err(format!(
                "unknown @compute kernel '{kernel}' (is .abi.json loaded?)"
            ));
        }
    };

    let groups = super::binds::plan_groups(&meta.bindings);
    let mut storage_idx = 0usize;
    let mut recreated = false;
    for bind in groups.iter().flat_map(|g| g.bindings.iter()) {
        if bind.kind == "storage" {
            let id = *buffer_ids.get(storage_idx).unwrap_or(&-1);
            storage_idx += 1;
            if let Some(entry) = st.buffers.get_mut(&id) {
                if ensure_gpu_buffer(
                    device,
                    queue,
                    entry,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                )? {
                    recreated = true;
                }
            } else {
                return Err(format!("missing buffer id {id}"));
            }
        }
    }

    if let Some((iid, _)) = indirect {
        if let Some(entry) = st.buffers.get_mut(&iid) {
            if ensure_gpu_buffer(
                device,
                queue,
                entry,
                wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::STORAGE,
            )? {
                recreated = true;
            }
        }
    }
    if recreated {
        clear_compute_bg_caches(st);
    }

    let mut storage_bufs: Vec<wgpu::Buffer> = Vec::new();
    let mut samplers: Vec<wgpu::Sampler> = Vec::new();
    let mut views: Vec<wgpu::TextureView> = Vec::new();
    let mut entry_plan: Vec<(u32, Vec<PlannedEntry>)> = Vec::new();
    let mut uniform_offset: Option<u32> = None;

    storage_idx = 0;
    let mut tex_idx = 0usize;
    let mut samp_idx = 0usize;

    for plan in &groups {
        let mut entries = Vec::new();
        for bind in &plan.bindings {
            match bind.kind.as_str() {
                "uniform" => {
                    let size = st
                        .compute_pipes
                        .get(kernel)
                        .map(|p| p.uniform_size)
                        .unwrap_or(0);
                    uniform_offset = Some(st.uniform_ring.push(queue, uniforms, size)?);
                    entries.push((bind.binding, 0, 0));
                }
                "storage" => {
                    let id = *buffer_ids.get(storage_idx).unwrap_or(&-1);
                    storage_idx += 1;
                    let g = st
                        .buffers
                        .get(&id)
                        .and_then(|b| b.gpu.clone())
                        .ok_or_else(|| format!("buffer {id} not on GPU"))?;
                    entries.push((bind.binding, storage_bufs.len(), 1));
                    storage_bufs.push(g);
                }
                "sampler" => {
                    let id = *sampler_ids.get(samp_idx).unwrap_or(&-1);
                    samp_idx += 1;
                    ensure_sampler(st, device, id)?;
                    let g = st.samplers.get(&id).unwrap().gpu.clone().unwrap();
                    entries.push((bind.binding, samplers.len(), 2));
                    samplers.push(g);
                }
                "texture" | "storage_texture" => {
                    let id = *texture_ids.get(tex_idx).unwrap_or(&-1);
                    tex_idx += 1;
                    ensure_texture(st, device, queue, id, bind.kind == "storage_texture")?;
                    let view = texture_view(st, id)?;
                    entries.push((bind.binding, views.len(), 3));
                    views.push(view);
                }
                _ => {}
            }
        }
        entry_plan.push((plan.group, entries));
    }

    let bg_key = super::state::ComputeBgKey {
        kernel: kernel.to_string(),
        buffer_ids: buffer_ids.to_vec(),
        texture_ids: texture_ids.to_vec(),
        sampler_ids: sampler_ids.to_vec(),
    };

    if !st
        .compute_pipes
        .get(kernel)
        .map(|p| p.bg_cache.contains_key(&bg_key))
        .unwrap_or(false)
    {
        let pipe = st
            .compute_pipes
            .get(kernel)
            .ok_or_else(|| format!("missing compute pipe '{kernel}'"))?;
        let needs_ub = entry_plan
            .iter()
            .any(|(_, entries)| entries.iter().any(|(_, _, k)| *k == 0));
        let uniform_size = pipe.uniform_size;
        // The window starts at 0 and the dispatch's dynamic offset moves it, so one bind group
        // serves every dispatch that shares these resources.
        let ring = st.uniform_ring.buffer();
        if needs_ub && ring.is_none() {
            return Err(format!("missing uniform buffer for '{kernel}'"));
        }
        let mut bgs = Vec::new();
        for (group, entries) in &entry_plan {
            let bgl = pipe.bgls.get(*group as usize).ok_or_else(|| {
                format!("kernel '{kernel}' has no layout for @group({group})")
            })?;
            let bg_entries: Vec<wgpu::BindGroupEntry<'_>> = entries
                .iter()
                .filter_map(|(binding, idx, kind)| {
                    Some(wgpu::BindGroupEntry {
                        binding: *binding,
                        resource: match kind {
                            0 => wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: ring?,
                                offset: 0,
                                size: std::num::NonZeroU64::new(u64::from(uniform_size)),
                            }),
                            1 => storage_bufs[*idx].as_entire_binding(),
                            2 => wgpu::BindingResource::Sampler(&samplers[*idx]),
                            _ => wgpu::BindingResource::TextureView(&views[*idx]),
                        },
                    })
                })
                .collect();
            bgs.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("dream-compute-bg"),
                layout: bgl,
                entries: &bg_entries,
            }));
        }
        st.compute_pipes
            .get_mut(kernel)
            .unwrap()
            .bg_cache
            .insert(bg_key.clone(), bgs);
    }

    let wg = meta.workgroup;
    let dispatch =
        |pass: &mut wgpu::ComputePass<'_>, st: &super::state::GpuState| -> Result<(), String> {
            let pipe = st
                .compute_pipes
                .get(kernel)
                .ok_or_else(|| format!("missing compute pipe '{kernel}'"))?;
            let bgs = pipe
                .bg_cache
                .get(&bg_key)
                .ok_or_else(|| "missing cached bind group".to_string())?;
            pass.set_pipeline(&pipe.pipeline);
            for ((i, bg), (_, entries)) in bgs.iter().enumerate().zip(&entry_plan) {
                let offsets: Vec<u32> = entries
                    .iter()
                    .filter(|(_, _, kind)| *kind == 0)
                    .filter_map(|_| uniform_offset)
                    .collect();
                pass.set_bind_group(i as u32, bg, &offsets);
            }
            if let Some((iid, off)) = indirect {
                let indirect_buf = st
                    .buffers
                    .get(&iid)
                    .and_then(|b| b.gpu.as_ref())
                    .ok_or_else(|| format!("missing indirect {iid}"))?;
                pass.dispatch_workgroups_indirect(indirect_buf, off.max(0) as u64);
            } else {
                let gx = (ex.max(1) as u32).div_ceil(wg[0].max(1));
                let gy = (ey.max(1) as u32).div_ceil(wg[1].max(1));
                let gz = (ez.max(1) as u32).div_ceil(wg[2].max(1));
                pass.dispatch_workgroups(gx.max(1), gy.max(1), gz.max(1));
            }
            Ok(())
        };
    if let Some(pass) = shared_pass {
        dispatch(pass, st)?;
    } else {
        let encoder = encoder.ok_or_else(|| "missing command encoder".to_string())?;
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("dream-cpass"),
            timestamp_writes: None,
        });
        dispatch(&mut pass, st)?;
    }
    Ok(())
}

pub(crate) fn texture_view(st: &mut super::state::GpuState, id: i32) -> Result<wgpu::TextureView, String> {
    let t = st
        .textures
        .get_mut(&id)
        .ok_or_else(|| format!("missing texture {id}"))?;
    if t.view.is_none() {
        let gpu = t
            .gpu
            .as_ref()
            .ok_or_else(|| format!("texture {id} has no GPU resource"))?
            .clone();
        t.view = Some(super::textures::default_view(t, &gpu));
    }
    Ok(t.view.as_ref().unwrap().clone())
}

fn ensure_pipeline(kernel: &str) -> Result<(), String> {
    let mut st = lock_state();
    if st.compute_pipes.contains_key(kernel) {
        return Ok(());
    }
    let device = st
        .device
        .as_ref()
        .ok_or_else(|| "GPU not initialized".to_string())?
        .clone();
    let meta = match st
        .abi
        .as_ref()
        .and_then(|a| a.kernels.iter().find(|k| k.name == kernel))
        .cloned()
    {
        Some(meta) => meta,
        None => {
            st.warn_if_gpu_abi_missing();
            return Err(format!(
                "unknown @compute kernel '{kernel}' (is .abi.json loaded?)"
            ));
        }
    };
    if meta.source.is_empty() {
        return Err(format!("kernel '{kernel}' has empty WGSL source"));
    }
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&meta.name),
        source: wgpu::ShaderSource::Wgsl(meta.source.clone().into()),
    });
    let groups = super::binds::plan_groups(&meta.bindings);
    let bgls = super::binds::create_group_layouts(
        &device,
        &groups,
        wgpu::ShaderStages::COMPUTE,
        "dream-compute",
        meta.uniform_size,
    );
    let bgl_refs: Vec<&wgpu::BindGroupLayout> = bgls.iter().collect();
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("dream-compute"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("dream-compute-pl"),
                bind_group_layouts: &bgl_refs,
                push_constant_ranges: &[],
            }),
        ),
        module: &module,
        entry_point: Some(&meta.entry),
        compilation_options: Default::default(),
        cache: None,
    });
    st.compute_pipes.insert(
        kernel.to_string(),
        ComputePipe {
            pipeline,
            bgls,
            uniform_size: meta.uniform_size,
            bg_cache: IndexMap::new(),
        },
    );
    Ok(())
}

pub(crate) fn ensure_sampler(
    st: &mut super::state::GpuState,
    device: &wgpu::Device,
    id: i32,
) -> Result<(), String> {
    let s = st
        .samplers
        .get_mut(&id)
        .ok_or_else(|| format!("missing sampler {id}"))?;
    if s.gpu.is_some() {
        return Ok(());
    }
    let compare = s.compare.map(formats::compare_function).transpose()?;
    // WebGPU requires all three filters to be linear before anisotropy takes effect, and rejects
    // an anisotropy above 1 otherwise.
    let anisotropy = if s.mag_filter == 1 && s.min_filter == 1 && s.mip_filter == 1 {
        s.max_anisotropy.max(1)
    } else {
        1
    };
    s.gpu = Some(device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("dream-sampler"),
        mag_filter: formats::filter_mode(s.mag_filter),
        min_filter: formats::filter_mode(s.min_filter),
        mipmap_filter: formats::filter_mode(s.mip_filter),
        address_mode_u: formats::address_mode(s.address[0]),
        address_mode_v: formats::address_mode(s.address[1]),
        address_mode_w: formats::address_mode(s.address[2]),
        lod_min_clamp: s.lod.0,
        lod_max_clamp: s.lod.1,
        compare,
        anisotropy_clamp: anisotropy,
        border_color: None,
    }));
    Ok(())
}

pub(crate) fn ensure_texture(
    st: &mut super::state::GpuState,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    id: i32,
    storage: bool,
) -> Result<(), String> {
    let mut recreated = false;
    {
        let t = st
            .textures
            .get_mut(&id)
            .ok_or_else(|| format!("missing texture {id}"))?;
        // A texture created without `storage_access` but bound to a `texture_storage_*` slot needs
        // `STORAGE_BINDING`, which cannot be added after the fact — drop and recreate.
        if storage && !t.storage {
            t.storage = true;
            if let Some(gpu) = t.gpu.take() {
                gpu.destroy();
            }
            t.view = None;
            t.dirty_cpu = true;
        }
        if t.gpu.is_some() && !t.dirty_cpu {
            return Ok(());
        }
        if t.dirty_cpu && t.mip_levels > 1 {
            // Base pixels changed; drop stale higher mips before recreate/upload.
            t.mip_levels = 1;
            if let Some(gpu) = t.gpu.take() {
                gpu.destroy();
            }
            t.view = None;
        }
        if t.gpu.is_none() {
            t.gpu = Some(device.create_texture(&wgpu::TextureDescriptor {
                label: Some("dream-tex"),
                size: wgpu::Extent3d {
                    width: t.width.max(1),
                    height: t.height.max(1),
                    depth_or_array_layers: t.layers.max(1),
                },
                mip_level_count: t.mip_levels.max(1),
                sample_count: t.sample_count.max(1),
                dimension: t.dimension,
                format: t.wgpu_format(),
                usage: texture_usage(t),
                view_formats: &[],
            }));
            t.view = None;
            recreated = true;
        }
        if t.dirty_cpu && !t.cpu.is_empty() {
            // `cpu` is only populated for linearly copyable color formats, so a mirror existing at
            // all means this upload is valid.
            let bpp = t.format.bytes_per_texel().unwrap_or(4);
            let tex = t.gpu.as_ref().unwrap();
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &t.cpu,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(t.width * bpp),
                    rows_per_image: Some(t.height),
                },
                wgpu::Extent3d {
                    width: t.width,
                    height: t.height,
                    // Uploads every layer: a cubemap's five other faces are in the mirror too, and
                    // only copying layer 0 left them undefined.
                    depth_or_array_layers: t.layers.max(1),
                },
            );
            t.dirty_cpu = false;
        }
    }
    if recreated {
        st.invalidate_blit_tex(id);
    }
    Ok(())
}

pub fn pass_begin(query_set: i32, ts_begin: i32, ts_end: i32) -> i32 {
    let mut st = lock_state();
    let id = st.alloc_id();
    st.passes.insert(
        id,
        ComputePassEntry {
            ops: Vec::new(),
            query_set,
            ts_begin,
            ts_end,
        },
    );
    id
}

pub fn pass_dispatch(
    pass_id: i32,
    kernel: String,
    buffer_ids: Vec<i32>,
    texture_ids: Vec<i32>,
    sampler_ids: Vec<i32>,
    ex: i32,
    ey: i32,
    ez: i32,
    uniforms: Vec<u8>,
) {
    let mut st = lock_state();
    if let Some(entry) = st.passes.get_mut(&pass_id) {
        entry.ops.push(PassOp::Dispatch {
            kernel,
            buffer_ids,
            texture_ids,
            sampler_ids,
            ex,
            ey,
            ez,
            uniforms,
        });
    }
}

pub fn pass_dispatch_indirect(
    pass_id: i32,
    kernel: String,
    buffer_ids: Vec<i32>,
    texture_ids: Vec<i32>,
    sampler_ids: Vec<i32>,
    indirect_id: i32,
    offset: i32,
) {
    let mut st = lock_state();
    if let Some(entry) = st.passes.get_mut(&pass_id) {
        entry.ops.push(PassOp::DispatchIndirect {
            kernel,
            buffer_ids,
            texture_ids,
            sampler_ids,
            indirect_id,
            offset,
        });
    }
}

pub fn shader_from_wgsl(source: String, entry: String) -> i32 {
    let mut st = lock_state();
    let id = st.alloc_id();
    st.shaders
        .insert(id, super::state::RawShader { source, entry });
    id
}

pub fn dispatch_shader(shader_id: i32, buffer_ids: &[i32], wx: i32, wy: i32, wz: i32) -> i32 {
    let (source, entry) = {
        let st = lock_state();
        match st.shaders.get(&shader_id) {
            Some(s) => (s.source.clone(), s.entry.clone()),
            None => return classify_err("unknown shader id / missing shader"),
        }
    };
    let name = format!("__raw_{shader_id}");
    {
        let mut st = lock_state();
        let mut abi = st.abi.clone().unwrap_or_default();
        abi.kernels.retain(|k| k.name != name);
        abi.kernels.push(super::abi::GpuKernelMeta {
            name: name.clone(),
            entry,
            workgroup: [wx.max(1) as u32, wy.max(1) as u32, wz.max(1) as u32],
            bindings: buffer_ids
                .iter()
                .enumerate()
                .map(|(i, _)| super::abi::GpuBindingMeta {
                    group: 0,
                    binding: i as u32,
                    kind: "storage".into(),
                    read_write: true,
                    ..Default::default()
                })
                .collect(),
            uniform_size: 0,
            source,
        });
        st.abi = Some(abi);
        st.compute_pipes.shift_remove(&name);
    }
    dispatch(
        &name,
        buffer_ids,
        &[],
        &[],
        wx.max(1),
        wy.max(1),
        wz.max(1),
        &[],
    )
}

pub fn pass_submit(pass_id: i32) -> i32 {
    let entry = {
        let mut st = lock_state();
        st.passes.swap_remove(&pass_id).unwrap_or_default()
    };
    let timed = entry.query_set >= 0;
    if entry.ops.is_empty() && !timed {
        return 0;
    }
    for op in &entry.ops {
        let kernel = match op {
            PassOp::Dispatch { kernel, .. } | PassOp::DispatchIndirect { kernel, .. } => kernel,
        };
        if let Err(e) = ensure_pipeline(kernel) {
            eprintln!("Dream gpuPassSubmit: {e}");
            lock_state().set_last_error(e.clone());
            return classify_err(&e);
        }
    }

    match (|| -> Result<(), String> {
        let mut st = lock_state();
        if !st.ready {
            return Err("GPU not initialized".into());
        }
        let device = st.device.as_ref().unwrap().clone();
        let queue = st.queue.as_ref().unwrap().clone();
        let ts_set = if timed {
            Some(
                st.query_sets
                    .get(&entry.query_set)
                    .and_then(|q| q.gpu.clone())
                    .ok_or_else(|| format!("unknown query set {}", entry.query_set))?,
            )
        } else {
            None
        };
        let batch: Vec<String> = entry
            .ops
            .iter()
            .map(|op| match op {
                PassOp::Dispatch { kernel, .. } | PassOp::DispatchIndirect { kernel, .. } => {
                    kernel.clone()
                }
            })
            .collect();
        begin_uniform_frame(&mut st, &device, batch);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("dream-pass"),
        });
        {
            let writes = ts_set.as_ref().and_then(|qs| {
                super::queries::compute_timestamp_writes(qs, entry.ts_begin, entry.ts_end)
            });
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("dream-batched-cpass"),
                timestamp_writes: writes,
            });
            for op in entry.ops {
                match op {
                    PassOp::Dispatch {
                        kernel,
                        buffer_ids,
                        texture_ids,
                        sampler_ids,
                        ex,
                        ey,
                        ez,
                        uniforms,
                    } => encode_op(
                        &mut st,
                        &device,
                        &queue,
                        None,
                        Some(&mut cpass),
                        &kernel,
                        &buffer_ids,
                        &texture_ids,
                        &sampler_ids,
                        ex,
                        ey,
                        ez,
                        &uniforms,
                        None,
                    )?,
                    PassOp::DispatchIndirect {
                        kernel,
                        buffer_ids,
                        texture_ids,
                        sampler_ids,
                        indirect_id,
                        offset,
                    } => encode_op(
                        &mut st,
                        &device,
                        &queue,
                        None,
                        Some(&mut cpass),
                        &kernel,
                        &buffer_ids,
                        &texture_ids,
                        &sampler_ids,
                        1,
                        1,
                        1,
                        &[],
                        Some((indirect_id, offset)),
                    )?,
                }
            }
        }
        if timed {
            if let Some(qs_entry) = st.query_sets.get(&entry.query_set) {
                super::queries::encode_resolve(&mut encoder, qs_entry)?;
            }
        }
        queue.submit(Some(encoder.finish()));
        let _ = device.poll(wgpu::Maintain::Poll);
        if let Some(e) = drain_uncaptured() {
            return Err(e);
        }
        Ok(())
    })() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Dream gpuPassSubmit: {e}");
            lock_state().set_last_error(e.clone());
            classify_err(&e)
        }
    }
}
