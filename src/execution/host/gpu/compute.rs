//! Compute pipeline cache + dispatch.
//! Argument counts mirror the Dream `@js` host ABI (intentional).

#![allow(clippy::too_many_arguments)]

use super::buffers::ensure_gpu_buffer;
use super::error::{classify_err, drain_uncaptured};
use super::state::{lock_state, ComputePipe, PassOp};
use indexmap::IndexMap;
use indexmap::IndexSet;

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
    match run(kernel, buffer_ids, texture_ids, sampler_ids, ex, ey, ez, uniforms, None) {
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
    reset_uniform_cursors(&mut st);
    let device = st.device.as_ref().unwrap().clone();
    let queue = st.queue.as_ref().unwrap().clone();
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

fn reset_uniform_cursors(st: &mut super::state::GpuState) {
    for pipe in st.compute_pipes.values_mut() {
        pipe.uniform_cursor = 0;
    }
}

fn clear_compute_bg_caches(st: &mut super::state::GpuState) {
    for pipe in st.compute_pipes.values_mut() {
        pipe.bg_cache.clear();
    }
}

fn alloc_uniform_slot(
    st: &mut super::state::GpuState,
    device: &wgpu::Device,
    kernel: &str,
) -> Result<(u32, wgpu::Buffer), String> {
    let pipe = st
        .compute_pipes
        .get_mut(kernel)
        .ok_or_else(|| format!("missing compute pipe '{kernel}'"))?;
    let slot = pipe.uniform_cursor;
    if slot >= pipe.uniform_pool.len() {
        pipe.uniform_pool.push(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dream-compute-uniform"),
            size: 256,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }
    let buf = pipe.uniform_pool[slot].clone();
    pipe.uniform_cursor = slot + 1;
    Ok((slot as u32, buf))
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
            return Err(format!("unknown @compute kernel '{kernel}' (is .abi.json loaded?)"));
        }
    };

    let mut storage_idx = 0usize;
    let mut seen = IndexSet::new();
    let mut recreated = false;
    for bind in &meta.bindings {
        if !seen.insert(bind.binding) {
            continue;
        }
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
    let mut entry_plan: Vec<(u32, usize, u8)> = Vec::new();
    let mut uniform_slot: u32 = 0;
    let mut uniform_buf: Option<wgpu::Buffer> = None;

    seen.clear();
    storage_idx = 0;
    let mut tex_idx = 0usize;
    let mut samp_idx = 0usize;

    for bind in &meta.bindings {
        if !seen.insert(bind.binding) {
            continue;
        }
        match bind.kind.as_str() {
            "uniform" => {
                let mut bytes = [0u8; 256];
                bytes[0..4].copy_from_slice(&ex.to_le_bytes());
                bytes[4..8].copy_from_slice(&ey.to_le_bytes());
                bytes[8..12].copy_from_slice(&ez.to_le_bytes());
                let n = uniforms.len().min(244);
                if n > 0 {
                    bytes[12..12 + n].copy_from_slice(&uniforms[..n]);
                }
                let (slot, buf) = alloc_uniform_slot(st, device, kernel)?;
                queue.write_buffer(&buf, 0, &bytes);
                uniform_slot = slot;
                uniform_buf = Some(buf);
                entry_plan.push((bind.binding, 0, 0));
            }
            "storage" => {
                let id = *buffer_ids.get(storage_idx).unwrap_or(&-1);
                storage_idx += 1;
                let g = st
                    .buffers
                    .get(&id)
                    .and_then(|b| b.gpu.clone())
                    .ok_or_else(|| format!("buffer {id} not on GPU"))?;
                let i = storage_bufs.len();
                storage_bufs.push(g);
                entry_plan.push((bind.binding, i, 1));
            }
            "sampler" => {
                let id = *sampler_ids.get(samp_idx).unwrap_or(&-1);
                samp_idx += 1;
                ensure_sampler(st, device, id)?;
                let g = st.samplers.get(&id).unwrap().gpu.clone().unwrap();
                let i = samplers.len();
                samplers.push(g);
                entry_plan.push((bind.binding, i, 2));
            }
            "texture" | "storage_texture" => {
                let id = *texture_ids.get(tex_idx).unwrap_or(&-1);
                tex_idx += 1;
                ensure_texture(st, device, queue, id, bind.kind == "storage_texture")?;
                let view = texture_view(st, id)?;
                let i = views.len();
                views.push(view);
                entry_plan.push((bind.binding, i, 3));
            }
            _ => {}
        }
    }

    let bg_key = super::state::ComputeBgKey {
        kernel: kernel.to_string(),
        buffer_ids: buffer_ids.to_vec(),
        texture_ids: texture_ids.to_vec(),
        sampler_ids: sampler_ids.to_vec(),
        uniform_slot,
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
        let needs_ub = entry_plan.iter().any(|(_, _, k)| *k == 0);
        let ub_owned = if needs_ub {
            Some(
                uniform_buf
                    .clone()
                    .or_else(|| pipe.uniform_pool.first().cloned())
                    .ok_or_else(|| format!("missing uniform buffer for '{kernel}'"))?,
            )
        } else {
            None
        };
        let bg_entries: Vec<wgpu::BindGroupEntry<'_>> = entry_plan
            .iter()
            .map(|(binding, idx, kind)| wgpu::BindGroupEntry {
                binding: *binding,
                resource: match kind {
                    0 => ub_owned.as_ref().unwrap().as_entire_binding(),
                    1 => storage_bufs[*idx].as_entire_binding(),
                    2 => wgpu::BindingResource::Sampler(&samplers[*idx]),
                    _ => wgpu::BindingResource::TextureView(&views[*idx]),
                },
            })
            .collect();
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dream-compute-bg"),
            layout: &pipe.bgl,
            entries: &bg_entries,
        });
        st.compute_pipes
            .get_mut(kernel)
            .unwrap()
            .bg_cache
            .insert(bg_key.clone(), bg);
    }

    let wg = meta.workgroup;
    let dispatch = |pass: &mut wgpu::ComputePass<'_>, st: &super::state::GpuState| -> Result<(), String> {
        let pipe = st
            .compute_pipes
            .get(kernel)
            .ok_or_else(|| format!("missing compute pipe '{kernel}'"))?;
        let bg = pipe
            .bg_cache
            .get(&bg_key)
            .ok_or_else(|| "missing cached bind group".to_string())?;
        pass.set_pipeline(&pipe.pipeline);
        pass.set_bind_group(0, bg, &[]);
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

fn texture_view(st: &mut super::state::GpuState, id: i32) -> Result<wgpu::TextureView, String> {
    let t = st
        .textures
        .get_mut(&id)
        .ok_or_else(|| format!("missing texture {id}"))?;
    if t.view.is_none() {
        let gpu = t
            .gpu
            .as_ref()
            .ok_or_else(|| format!("texture {id} has no GPU resource"))?;
        t.view = Some(gpu.create_view(&Default::default()));
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
            return Err(format!("unknown @compute kernel '{kernel}' (is .abi.json loaded?)"));
        }
    };
    if meta.source.is_empty() {
        return Err(format!("kernel '{kernel}' has empty WGSL source"));
    }
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&meta.name),
        source: wgpu::ShaderSource::Wgsl(meta.source.clone().into()),
    });
    let mut seen = IndexSet::new();
    let mut entries = Vec::new();
    for b in &meta.bindings {
        if !seen.insert(b.binding) {
            continue;
        }
        let visibility = wgpu::ShaderStages::COMPUTE;
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
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: b.binding,
            visibility,
            ty,
            count: None,
        });
    }
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("dream-compute-bgl"),
        entries: &entries,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("dream-compute"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("dream-compute-pl"),
                bind_group_layouts: &[&bgl],
                push_constant_ranges: &[],
            }),
        ),
        module: &module,
        entry_point: Some(&meta.entry),
        compilation_options: Default::default(),
        cache: None,
    });
    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dream-compute-uniform"),
        size: 256,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    st.compute_pipes.insert(
        kernel.to_string(),
        ComputePipe {
            pipeline,
            bgl,
            uniform_pool: vec![uniform_buf],
            uniform_cursor: 0,
            bg_cache: IndexMap::new(),
        },
    );
    Ok(())
}

fn ensure_sampler(
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
    let filter = if s.filter == 1 {
        wgpu::FilterMode::Linear
    } else {
        wgpu::FilterMode::Nearest
    };
    let address = match s.address {
        1 => wgpu::AddressMode::Repeat,
        2 => wgpu::AddressMode::MirrorRepeat,
        _ => wgpu::AddressMode::ClampToEdge,
    };
    s.gpu = Some(device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: if s.mip_filter == 1 {
            wgpu::FilterMode::Linear
        } else {
            wgpu::FilterMode::Nearest
        },
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        ..Default::default()
    }));
    Ok(())
}

fn ensure_texture(
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
        if storage {
            t.storage = true;
        }
        if t.gpu.is_some() && !t.dirty_cpu {
            return Ok(());
        }
        if t.gpu.is_none() {
            let mut usage = wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT;
            if t.storage {
                usage |= wgpu::TextureUsages::STORAGE_BINDING;
            }
            t.gpu = Some(device.create_texture(&wgpu::TextureDescriptor {
                label: Some("dream-tex"),
                size: wgpu::Extent3d {
                    width: t.width.max(1),
                    height: t.height.max(1),
                    depth_or_array_layers: t.layers.max(1),
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: t.format,
                usage,
                view_formats: &[],
            }));
            t.view = None;
            recreated = true;
        }
        if t.dirty_cpu && !t.cpu.is_empty() && !t.depth {
            let bpp = if t.format == wgpu::TextureFormat::Rgba16Float {
                8
            } else {
                4
            };
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
                    depth_or_array_layers: 1,
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

pub fn pass_begin() -> i32 {
    let mut st = lock_state();
    let id = st.alloc_id();
    st.passes.insert(id, Vec::new());
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
    if let Some(ops) = st.passes.get_mut(&pass_id) {
        ops.push(PassOp::Dispatch {
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
    if let Some(ops) = st.passes.get_mut(&pass_id) {
        ops.push(PassOp::DispatchIndirect {
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
    st.shaders.insert(
        id,
        super::state::RawShader { source, entry },
    );
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
                    name: format!("b{i}"),
                    binding: i as u32,
                    kind: "storage".into(),
                    type_: "f32".into(),
                    read_write: true,
                    atomic: false,
                })
                .collect(),
            source,
        });
        st.abi = Some(abi);
        st.compute_pipes.shift_remove(&name);
    }
    dispatch(&name, buffer_ids, &[], &[], wx.max(1), wy.max(1), wz.max(1), &[])
}

pub fn pass_submit(pass_id: i32) -> i32 {
    let ops = {
        let mut st = lock_state();
        st.passes.swap_remove(&pass_id).unwrap_or_default()
    };
    if ops.is_empty() {
        return 0;
    }
    for op in &ops {
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
        reset_uniform_cursors(&mut st);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("dream-pass"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("dream-batched-cpass"),
                timestamp_writes: None,
            });
            for op in ops {
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
