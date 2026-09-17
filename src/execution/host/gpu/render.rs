//! Render pipelines and explicit bind groups. Draws are recorded by the app and replayed in
//! `encoder::submit`.
//! Argument counts mirror the Dream `@js` host ABI (intentional).

#![allow(clippy::too_many_arguments)]

use super::error::classify_err;
use super::state::{lock_state, BindGroupEntry, RenderPipe, RenderPipeBuild};
use indexmap::IndexMap;

/// Unknown spellings cannot reach here from a compiled program — the emitter only writes names
/// from the shared registry — so an unrecognized one falls back rather than failing the pipeline.
fn vertex_format(s: &str) -> wgpu::VertexFormat {
    super::formats::vertex_format_named(s)
        .map(|(f, _)| f)
        .unwrap_or(wgpu::VertexFormat::Float32x4)
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
    let abi = match st.abi.as_ref() {
        Some(abi) => abi,
        None => {
            st.warn_if_gpu_abi_missing();
            return Err("no abi.gpu loaded".to_string());
        }
    };
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
    let groups = super::binds::plan_groups(&all_binds);
    let visibility = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
    // Both stages fold their uniform parameters into one shared block, so a stage that declares no
    // uniforms reports 0 and the other stage's size is the pipeline's.
    let uniform_size = vs.uniform_size.max(fs.uniform_size);
    if vs.uniform_size != 0 && fs.uniform_size != 0 && vs.uniform_size != fs.uniform_size {
        return Err(format!(
            "@vertex '{vs_name}' and @fragment '{fs_name}' declare different uniform blocks \
             ({} vs {} bytes); the stages of one pipeline share a single block",
            vs.uniform_size, fs.uniform_size
        ));
    }
    let bgls = super::binds::create_group_layouts(
        &device,
        &groups,
        visibility,
        "dream-render",
        uniform_size,
    );
    let bgl_refs: Vec<&wgpu::BindGroupLayout> = bgls.iter().collect();
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("dream-render-pl"),
        bind_group_layouts: &bgl_refs,
        push_constant_ranges: &[],
    });

    let build = RenderPipeBuild {
        vs_mod,
        fs_mod,
        vs_entry: vs.entry.clone(),
        fs_entry: fs.entry.clone(),
        layout: pl,
        vertex_buffers: vs
            .vertex_buffers
            .iter()
            .map(|b| super::state::VertexBufferBuild {
                stride: b.stride,
                step_mode: if b.step_mode == "instance" {
                    wgpu::VertexStepMode::Instance
                } else {
                    wgpu::VertexStepMode::Vertex
                },
                attributes: b
                    .attributes
                    .iter()
                    .map(|a| wgpu::VertexAttribute {
                        format: vertex_format(&a.format),
                        offset: a.offset as u64,
                        shader_location: a.location,
                    })
                    .collect(),
            })
            .collect(),
        color_targets: fs.color_targets.max(1),
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
        blend: if blend_enabled {
            Some(wgpu::BlendState::ALPHA_BLENDING)
        } else {
            None
        },
        depth_stencil: if depth_enabled {
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: depth_write,
                depth_compare: compare(depth_compare),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            })
        } else {
            None
        },
        sample_count,
    };

    let format = st.render_format;
    let pipeline = build_pipeline(&device, &build, format);

    let id = st.alloc_id();
    let mut variants = IndexMap::new();
    variants.insert(format, pipeline);
    st.render_pipes.insert(
        id,
        RenderPipe {
            variants,
            build,
            bgls,
            groups,
            uniform_size,
            depth_enabled,
            sample_count,
        },
    );
    Ok(id)
}

fn build_pipeline(
    device: &wgpu::Device,
    b: &RenderPipeBuild,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let vertex_buffers: Vec<wgpu::VertexBufferLayout<'_>> = b
        .vertex_buffers
        .iter()
        .filter(|v| v.stride > 0 && !v.attributes.is_empty())
        .map(|v| wgpu::VertexBufferLayout {
            array_stride: u64::from(v.stride),
            step_mode: v.step_mode,
            attributes: &v.attributes,
        })
        .collect();
    let targets: Vec<Option<wgpu::ColorTargetState>> = (0..b.color_targets)
        .map(|_| {
            Some(wgpu::ColorTargetState {
                format,
                blend: b.blend,
                write_mask: wgpu::ColorWrites::ALL,
            })
        })
        .collect();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("dream-render"),
        layout: Some(&b.layout),
        vertex: wgpu::VertexState {
            module: &b.vs_mod,
            entry_point: Some(&b.vs_entry),
            compilation_options: Default::default(),
            buffers: &vertex_buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module: &b.fs_mod,
            entry_point: Some(&b.fs_entry),
            compilation_options: Default::default(),
            targets: &targets,
        }),
        primitive: wgpu::PrimitiveState {
            topology: b.topology,
            front_face: b.front_face,
            cull_mode: b.cull_mode,
            ..Default::default()
        },
        depth_stencil: b.depth_stencil.clone(),
        multisample: wgpu::MultisampleState {
            count: b.sample_count,
            ..Default::default()
        },
        multiview: None,
        cache: None,
    })
}

/// The pipeline variant matching `format`, minted and cached on first use. A Dream pipeline handle
/// is format-agnostic; the color-target format is a property of the draw, not of the handle.
pub(crate) fn pipeline_for_format(
    st: &mut super::state::GpuState,
    device: &wgpu::Device,
    pipeline_id: i32,
    format: wgpu::TextureFormat,
) -> Result<wgpu::RenderPipeline, String> {
    let rp = st
        .render_pipes
        .get_mut(&pipeline_id)
        .ok_or_else(|| format!("unknown pipeline {pipeline_id}"))?;
    if let Some(p) = rp.variants.get(&format) {
        return Ok(p.clone());
    }
    let built = build_pipeline(device, &rp.build, format);
    rp.variants.insert(format, built.clone());
    Ok(built)
}

pub fn pipeline_destroy(id: i32) {
    let mut st = lock_state();
    st.render_pipes.shift_remove(&id);
    st.render_bg_cache.retain(|k, _| k.pipeline_id != id);
}

/// Registers a reusable resource set for one `@group` of `pipeline_id`.
///
/// Validation happens here (rather than at first draw) so a bad material fails at load time: the
/// group must exist in the pipeline's layout and the supplied ids must cover its bindings.
pub fn bind_group_create(
    pipeline_id: i32,
    group: i32,
    buffer_ids: &[i32],
    texture_ids: &[i32],
    sampler_ids: &[i32],
) -> i32 {
    let mut st = lock_state();
    if !st.ready {
        eprintln!("Dream gpuBindGroupCreate: GPU not initialized");
        return -classify_err("GPU not initialized");
    }
    let plan = match st.render_pipes.get(&pipeline_id) {
        Some(rp) => rp.groups.iter().find(|g| g.group == group as u32).cloned(),
        None => {
            eprintln!("Dream gpuBindGroupCreate: unknown pipeline {pipeline_id}");
            return -classify_err("unknown pipeline");
        }
    };
    let Some(plan) = plan else {
        eprintln!("Dream gpuBindGroupCreate: pipeline {pipeline_id} declares no @group({group})");
        return -classify_err("validation: unknown bind group");
    };
    let (mut want_buf, mut want_tex, mut want_samp) = (0usize, 0usize, 0usize);
    for b in &plan.bindings {
        match b.kind.as_str() {
            "storage" => want_buf += 1,
            "sampler" => want_samp += 1,
            "uniform" => {}
            _ => want_tex += 1,
        }
    }
    if buffer_ids.len() < want_buf || texture_ids.len() < want_tex || sampler_ids.len() < want_samp
    {
        eprintln!(
            "Dream gpuBindGroupCreate: @group({group}) needs {want_buf} buffer(s), {want_tex} texture(s), {want_samp} sampler(s); got {}, {}, {}",
            buffer_ids.len(),
            texture_ids.len(),
            sampler_ids.len()
        );
        return -classify_err("validation: bind group arity");
    }
    let id = st.alloc_id();
    st.bind_groups.insert(
        id,
        BindGroupEntry {
            pipeline_id,
            group: group as u32,
            buffer_ids: buffer_ids.to_vec(),
            texture_ids: texture_ids.to_vec(),
            sampler_ids: sampler_ids.to_vec(),
        },
    );
    id
}

pub fn bind_group_destroy(id: i32) {
    let mut st = lock_state();
    st.bind_groups.shift_remove(&id);
    st.render_bg_cache.retain(|k, _| k.bind_group_id != id);
}
