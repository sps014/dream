//! Render pipelines + draw (offscreen / surface color target).
//! Argument counts mirror the Dream `@js` host ABI (intentional).

#![allow(clippy::too_many_arguments)]

use super::abi::GpuBindingMeta;
use super::buffers::ensure_gpu_buffer;
use super::error::{classify_err, classify_surface_error, drain_uncaptured};
use super::state::{lock_state, RenderPipe};
use indexmap::IndexSet;

fn vertex_format(s: &str) -> wgpu::VertexFormat {
    match s {
        "float32" => wgpu::VertexFormat::Float32,
        "float32x2" => wgpu::VertexFormat::Float32x2,
        "float32x3" => wgpu::VertexFormat::Float32x3,
        "float32x4" => wgpu::VertexFormat::Float32x4,
        "sint32" => wgpu::VertexFormat::Sint32,
        "uint32" => wgpu::VertexFormat::Uint32,
        _ => wgpu::VertexFormat::Float32x4,
    }
}

fn topology(t: i32) -> wgpu::PrimitiveTopology {
    match t {
        1 => wgpu::PrimitiveTopology::TriangleStrip,
        2 => wgpu::PrimitiveTopology::LineList,
        3 => wgpu::PrimitiveTopology::LineStrip,
        4 => wgpu::PrimitiveTopology::PointList,
        _ => wgpu::PrimitiveTopology::TriangleList,
    }
}

fn cull(c: i32) -> wgpu::Face {
    match c {
        1 => wgpu::Face::Front,
        _ => wgpu::Face::Back,
    }
}

fn compare(c: i32) -> wgpu::CompareFunction {
    match c {
        1 => wgpu::CompareFunction::LessEqual,
        2 => wgpu::CompareFunction::Greater,
        3 => wgpu::CompareFunction::GreaterEqual,
        4 => wgpu::CompareFunction::Always,
        5 => wgpu::CompareFunction::Never,
        _ => wgpu::CompareFunction::Less,
    }
}

pub fn pipeline_create_ex(
    vs_name: &str,
    fs_name: &str,
    topology_i: i32,
    cull_mode: i32,
    front_face: i32,
    depth_enabled: i32,
    depth_write: i32,
    depth_compare: i32,
    blend_enabled: i32,
    sample_count: i32,
) -> i32 {
    match create_inner(
        vs_name,
        fs_name,
        topology_i,
        cull_mode,
        front_face,
        depth_enabled != 0,
        depth_write != 0,
        depth_compare,
        blend_enabled != 0,
        sample_count.max(1) as u32,
    ) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Dream gpuRenderPipelineCreateEx: {e}");
            -classify_err(&e)
        }
    }
}

fn create_inner(
    vs_name: &str,
    fs_name: &str,
    topology_i: i32,
    cull_mode: i32,
    front_face: i32,
    depth_enabled: bool,
    depth_write: bool,
    depth_compare: i32,
    blend_enabled: bool,
    sample_count: u32,
) -> Result<i32, String> {
    let mut st = lock_state();
    if !st.ready {
        return Err("GPU not initialized".into());
    }
    let device = st.device.as_ref().unwrap().clone();
    let abi = st
        .abi
        .as_ref()
        .ok_or_else(|| "no abi.gpu loaded".to_string())?;
    let vs = abi
        .shaders
        .iter()
        .find(|s| s.name == vs_name && s.stage == "vertex")
        .cloned()
        .ok_or_else(|| format!("unknown @vertex '{vs_name}'"))?;
    let fs = abi
        .shaders
        .iter()
        .find(|s| s.name == fs_name && s.stage == "fragment")
        .cloned()
        .ok_or_else(|| format!("unknown @fragment '{fs_name}'"))?;

    let vs_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&vs.name),
        source: wgpu::ShaderSource::Wgsl(vs.source.clone().into()),
    });
    let fs_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&fs.name),
        source: wgpu::ShaderSource::Wgsl(fs.source.clone().into()),
    });

    let mut all_binds = Vec::new();
    all_binds.extend(vs.bindings.iter().cloned());
    all_binds.extend(fs.bindings.iter().cloned());
    let mut seen = IndexSet::new();
    let mut layout_entries = Vec::new();
    let mut uniform_bindings = Vec::new();
    for b in &all_binds {
        if !seen.insert(b.binding) {
            continue;
        }
        if b.kind == "uniform" {
            uniform_bindings.push(b.binding);
        }
        layout_entries.push(render_layout_entry(b));
    }
    let bgl = if layout_entries.is_empty() {
        None
    } else {
        Some(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dream-render-bgl"),
            entries: &layout_entries,
        }))
    };
    let pl = match &bgl {
        Some(l) => device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dream-render-pl"),
            bind_group_layouts: &[l],
            push_constant_ranges: &[],
        }),
        None => device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dream-render-pl"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        }),
    };

    let attribs: Vec<wgpu::VertexAttribute> = vs
        .vertex_layout
        .iter()
        .map(|a| wgpu::VertexAttribute {
            format: vertex_format(&a.format),
            offset: a.offset as u64,
            shader_location: a.location,
        })
        .collect();
    let vertex_buffers = if vs.vertex_stride > 0 && !attribs.is_empty() {
        vec![wgpu::VertexBufferLayout {
            array_stride: vs.vertex_stride as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attribs,
        }]
    } else {
        Vec::new()
    };

    let format = st.render_format;
    let targets: Vec<Option<wgpu::ColorTargetState>> = (0..fs.color_targets.max(1))
        .map(|_| {
            Some(wgpu::ColorTargetState {
                format,
                blend: if blend_enabled {
                    Some(wgpu::BlendState::ALPHA_BLENDING)
                } else {
                    None
                },
                write_mask: wgpu::ColorWrites::ALL,
            })
        })
        .collect();

    let depth_stencil = if depth_enabled {
        Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24Plus,
            depth_write_enabled: depth_write,
            depth_compare: compare(depth_compare),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        })
    } else {
        None
    };

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("dream-render"),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module: &vs_mod,
            entry_point: Some(&vs.entry),
            compilation_options: Default::default(),
            buffers: &vertex_buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module: &fs_mod,
            entry_point: Some(&fs.entry),
            compilation_options: Default::default(),
            targets: &targets,
        }),
        primitive: wgpu::PrimitiveState {
            topology: topology(topology_i),
            front_face: if front_face == 1 {
                wgpu::FrontFace::Cw
            } else {
                wgpu::FrontFace::Ccw
            },
            cull_mode: if cull_mode == 0 {
                None
            } else {
                Some(cull(cull_mode))
            },
            ..Default::default()
        },
        depth_stencil,
        multisample: wgpu::MultisampleState {
            count: sample_count,
            ..Default::default()
        },
        multiview: None,
        cache: None,
    });

    let uniform_buf = if bgl.is_some() && !uniform_bindings.is_empty() {
        Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dream-draw-uniform"),
            size: 256,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }))
    } else {
        None
    };

    let id = st.alloc_id();
    st.render_pipes.insert(
        id,
        RenderPipe {
            pipeline,
            bgl,
            uniform_bindings,
            uniform_buf,
            depth_enabled,
            sample_count,
            format,
        },
    );
    Ok(id)
}

fn render_layout_entry(b: &GpuBindingMeta) -> wgpu::BindGroupLayoutEntry {
    let visibility = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
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

pub fn draw_ex(
    surface_id: i32,
    pipeline_id: i32,
    vertex_buffer_id: i32,
    vertex_count: i32,
    instance_count: i32,
    uniforms: &[u8],
    clear: [f32; 4],
    depth_texture_id: i32,
    load_op: i32,
    indexed: Option<(i32, i32)>,
) -> i32 {
    match draw_inner(
        surface_id,
        pipeline_id,
        vertex_buffer_id,
        vertex_count,
        instance_count,
        uniforms,
        clear,
        depth_texture_id,
        load_op,
        indexed,
    ) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Dream gpuRenderDraw: {e}");
            classify_err(&e)
        }
    }
}

fn draw_inner(
    surface_id: i32,
    pipeline_id: i32,
    vertex_buffer_id: i32,
    vertex_count: i32,
    instance_count: i32,
    uniforms: &[u8],
    clear: [f32; 4],
    depth_texture_id: i32,
    load_op: i32,
    indexed: Option<(i32, i32)>,
) -> Result<(), String> {
    let mut st = lock_state();
    if !st.ready {
        return Err("GPU not initialized".into());
    }
    let device = st.device.as_ref().unwrap().clone();
    let queue = st.queue.as_ref().unwrap().clone();
    let rp = st
        .render_pipes
        .get(&pipeline_id)
        .ok_or_else(|| format!("unknown pipeline {pipeline_id}"))?;
    let depth_enabled = rp.depth_enabled;
    let format = rp.format;
    let sample_count = rp.sample_count;

    // Prefer drawing straight into the window swapchain (avoids an extra copy on present).
    let use_swapchain = {
        let surf = st
            .surfaces
            .get(&surface_id)
            .ok_or_else(|| format!("unknown surface {surface_id}"))?;
        surf.surface.is_some() && surf.config.is_some() && sample_count <= 1
    };

    let (color_view, pending_frame) = if use_swapchain {
        let surf = st.surfaces.get_mut(&surface_id).unwrap();
        // Reuse an already-acquired frame so multi-draw + load_op=Load works.
        let frame = if let Some(existing) = surf.pending_frame.take() {
            existing
        } else {
            let surface = surf.surface.as_ref().unwrap();
            match surface.get_current_texture() {
                Ok(f) => f,
                Err(e) => {
                    // One reconfigure retry for Lost/Outdated.
                    if matches!(
                        e,
                        wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated
                    ) {
                        if let Some(cfg) = surf.config.clone() {
                            surface.configure(&device, &cfg);
                        }
                        surface
                            .get_current_texture()
                            .map_err(|e2| {
                                format!(
                                    "surface acquire failed ({})",
                                    match classify_surface_error(&e2) {
                                        c if c == super::state::ERR_TIMEOUT => "timeout",
                                        c if c == super::state::ERR_VALIDATION => "validation",
                                        _ => "other",
                                    }
                                )
                            })?
                    } else {
                        return Err(format!(
                            "surface acquire failed ({})",
                            match classify_surface_error(&e) {
                                c if c == super::state::ERR_TIMEOUT => "timeout",
                                c if c == super::state::ERR_VALIDATION => "validation",
                                _ => "other",
                            }
                        ));
                    }
                }
            }
        };
        let view = frame.texture.create_view(&Default::default());
        (view, Some(frame))
    } else {
        let surf = st
            .surfaces
            .get_mut(&surface_id)
            .ok_or_else(|| format!("unknown surface {surface_id}"))?;
        if surf.color.is_none() {
            surf.color = Some(device.create_texture(&wgpu::TextureDescriptor {
                label: Some("dream-surface-color"),
                size: wgpu::Extent3d {
                    width: surf.width.max(1),
                    height: surf.height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            }));
        }
        (
            surf.color
                .as_ref()
                .unwrap()
                .create_view(&Default::default()),
            None,
        )
    };

    {
        let surf = st.surfaces.get_mut(&surface_id).unwrap();
        if depth_enabled && surf.depth.is_none() && depth_texture_id < 0 {
            surf.depth = Some(device.create_texture(&wgpu::TextureDescriptor {
                label: Some("dream-surface-depth"),
                size: wgpu::Extent3d {
                    width: surf.width.max(1),
                    height: surf.height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth24Plus,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            }));
        }
    }

    if let Some(entry) = st.buffers.get_mut(&vertex_buffer_id) {
        ensure_gpu_buffer(
            &device,
            &queue,
            entry,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        )?;
    }
    if let Some((ib, _)) = indexed {
        if let Some(entry) = st.buffers.get_mut(&ib) {
            ensure_gpu_buffer(
                &device,
                &queue,
                entry,
                wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            )?;
        }
    }

    let depth_view = if depth_enabled {
        if depth_texture_id >= 0 {
            // Ensure external depth tex.
            if let Some(t) = st.textures.get_mut(&depth_texture_id) {
                if t.gpu.is_none() {
                    t.gpu = Some(device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("dream-depth"),
                        size: wgpu::Extent3d {
                            width: t.width.max(1),
                            height: t.height.max(1),
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Depth24Plus,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    }));
                }
            }
            st.textures
                .get(&depth_texture_id)
                .and_then(|t| t.gpu.as_ref())
                .map(|t| t.create_view(&Default::default()))
        } else {
            st.surfaces
                .get(&surface_id)
                .and_then(|s| s.depth.as_ref())
                .map(|t| t.create_view(&Default::default()))
        }
    } else {
        None
    };

    let load = if load_op == 1 {
        wgpu::LoadOp::Load
    } else {
        wgpu::LoadOp::Clear(wgpu::Color {
            r: clear[0] as f64,
            g: clear[1] as f64,
            b: clear[2] as f64,
            a: clear[3] as f64,
        })
    };

    // Uniform bind group — binding indices come from abi.gpu (e.g. ocean uses @binding(3)).
    let uniform_bindings = st
        .render_pipes
        .get(&pipeline_id)
        .map(|rp| rp.uniform_bindings.clone())
        .unwrap_or_default();
    let needs_bg = !uniform_bindings.is_empty()
        && st
            .render_pipes
            .get(&pipeline_id)
            .and_then(|rp| rp.bgl.as_ref())
            .is_some();
    if needs_bg {
        let mut bytes = [0u8; 256];
        let n = uniforms.len().min(256);
        if n > 0 {
            bytes[..n].copy_from_slice(&uniforms[..n]);
        }
        let ub = st
            .render_pipes
            .get(&pipeline_id)
            .and_then(|rp| rp.uniform_buf.as_ref())
            .ok_or_else(|| "missing persistent draw uniform buffer".to_string())?;
        queue.write_buffer(ub, 0, &bytes);
    }

    let rp = st.render_pipes.get(&pipeline_id).unwrap();
    let bg = if needs_bg {
        let bgl = rp.bgl.as_ref().unwrap();
        let ub = rp.uniform_buf.as_ref().unwrap();
        let entries: Vec<wgpu::BindGroupEntry<'_>> = uniform_bindings
            .iter()
            .map(|binding| wgpu::BindGroupEntry {
                binding: *binding,
                resource: ub.as_entire_binding(),
            })
            .collect();
        Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dream-draw-bg"),
            layout: bgl,
            entries: &entries,
        }))
    } else {
        None
    };

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("dream-draw"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dream-rpass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: depth_view.as_ref().map(|dv| {
                wgpu::RenderPassDepthStencilAttachment {
                    view: dv,
                    depth_ops: Some(wgpu::Operations {
                        load: if load_op == 1 {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(1.0)
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&rp.pipeline);
        if let Some(bg) = &bg {
            pass.set_bind_group(0, bg, &[]);
        }
        if let Some(vb) = st.buffers.get(&vertex_buffer_id).and_then(|b| b.gpu.as_ref()) {
            pass.set_vertex_buffer(0, vb.slice(..));
        }
        let instances = instance_count.max(1) as u32;
        if let Some((ib_id, index_count)) = indexed {
            let ib = st
                .buffers
                .get(&ib_id)
                .and_then(|b| b.gpu.as_ref())
                .ok_or_else(|| format!("missing index buffer {ib_id}"))?;
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..index_count.max(0) as u32, 0, 0..instances);
        } else {
            pass.draw(0..vertex_count.max(0) as u32, 0..instances);
        }
    }
    queue.submit(Some(encoder.finish()));
    if let Some(frame) = pending_frame {
        st.surfaces
            .get_mut(&surface_id)
            .unwrap()
            .pending_frame = Some(frame);
    }
    // Don't stall the CPU on GPU completion every draw — vsync on present paces frames.
    let _ = device.poll(wgpu::Maintain::Poll);
    if let Some(e) = drain_uncaptured() {
        st.set_last_error(e.clone());
        return Err(e);
    }
    Ok(())
}
