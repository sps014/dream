//! Turns decoded records into wgpu calls.
//!
//! Resources are resolved into an owned plan *before* `begin_render_pass`, because an open
//! `wgpu::RenderPass` borrows the encoder and every resource it binds, which rules out touching
//! `GpuState` mid-pass.

use super::super::buffers::ensure_gpu_buffer;
use super::super::state::{lock_state, GpuState};
use super::attach::{self, Attachments};
use super::binds;
use super::decode::{self, PassDesc, Record};
use indexmap::IndexMap;

enum Step {
    Pipeline(wgpu::RenderPipeline),
    /// Group index, group, and the dynamic offsets its uniform binding needs.
    BindGroups(Vec<(u32, wgpu::BindGroup, Vec<u32>)>),
    VertexBuffer {
        slot: u32,
        buffer: wgpu::Buffer,
    },
    IndexBuffer {
        buffer: wgpu::Buffer,
        fmt: wgpu::IndexFormat,
    },
    Viewport {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        min_depth: f32,
        max_depth: f32,
    },
    Scissor {
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    },
    Draw {
        vertices: std::ops::Range<u32>,
        instances: std::ops::Range<u32>,
    },
    DrawIndexed {
        indices: std::ops::Range<u32>,
        base_vertex: i32,
        instances: std::ops::Range<u32>,
    },
    DrawIndirect {
        buffer: wgpu::Buffer,
        offset: u64,
    },
    DrawIndexedIndirect {
        buffer: wgpu::Buffer,
        offset: u64,
    },
    WriteTimestamp {
        query_set: wgpu::QuerySet,
        index: u32,
    },
}

struct Pass {
    attachments: Attachments,
    steps: Vec<Step>,
    query_set: Option<wgpu::QuerySet>,
    ts_begin: i32,
    ts_end: i32,
}

enum FrameOp {
    Pass(Pass),
    WriteTimestamp {
        query_set: wgpu::QuerySet,
        index: u32,
    },
}

fn index_format(fmt: i32) -> wgpu::IndexFormat {
    if fmt == 1 {
        wgpu::IndexFormat::Uint16
    } else {
        wgpu::IndexFormat::Uint32
    }
}

fn gpu_buffer(
    st: &mut GpuState,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    id: i32,
    usage: wgpu::BufferUsages,
) -> Result<wgpu::Buffer, String> {
    let entry = st
        .buffers
        .get_mut(&id)
        .ok_or_else(|| format!("unknown buffer {id}"))?;
    ensure_gpu_buffer(device, queue, entry, usage | wgpu::BufferUsages::COPY_DST)?;
    st.buffers
        .get(&id)
        .and_then(|b| b.gpu.clone())
        .ok_or_else(|| format!("buffer {id} not on GPU"))
}

/// Resource state carried across the records of one pass, mirroring WebGPU's pass-scoped state.
#[derive(Default)]
struct Bound {
    pipeline: i32,
    explicit: IndexMap<u32, i32>,
    list: Option<binds::BindList>,
    uniform_offset: Option<u32>,
}

fn plan_pass(
    st: &mut GpuState,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    desc: &PassDesc,
    records: &[Record],
) -> Result<Pass, String> {
    // The attachments' sample count must match the pipeline's, and the pipeline variant is chosen
    // by the attachment format — so the pipeline has to be known before either can be resolved.
    let first_pipeline = records
        .iter()
        .find_map(|r| match r {
            Record::SetPipeline(id) => Some(*id),
            _ => None,
        })
        .ok_or("render pass issued no set_pipeline")?;
    let (sample_count, needs_depth) = st
        .render_pipes
        .get(&first_pipeline)
        .map(|p| (p.sample_count, p.depth_enabled))
        .ok_or_else(|| format!("unknown pipeline {first_pipeline}"))?;
    let attachments = attach::resolve(st, device, queue, desc, sample_count)?;
    if needs_depth && attachments.depth.is_none() {
        return Err(format!(
            "pipeline {first_pipeline} was built with depth testing; the render target needs a \
             depth attachment (`depth(tex, op)` or `surface_depth(op)`)"
        ));
    }

    let mut steps = Vec::new();
    let mut bound = Bound {
        pipeline: -1,
        ..Default::default()
    };

    for rec in records {
        match rec {
            Record::SetPipeline(id) => {
                let pipe = super::super::render::pipeline_for_format(
                    st,
                    device,
                    *id,
                    attachments.format,
                )?;
                bound.pipeline = *id;
                // A pipeline switch invalidates the previous pipeline's group layouts.
                bound.explicit.clear();
                bound.list = None;
                steps.push(Step::Pipeline(pipe));
            }
            Record::SetBindGroup { group, id } => {
                bound.explicit.insert(*group, *id);
            }
            Record::SetBindList {
                buffers,
                textures,
                samplers,
            } => {
                bound.list = Some(binds::BindList::new(
                    buffers.clone(),
                    textures.clone(),
                    samplers.clone(),
                ));
            }
            Record::SetUniforms(bytes) => {
                if bound.pipeline < 0 {
                    return Err("set_uniforms before set_pipeline".into());
                }
                bound.uniform_offset = binds::push_uniforms(st, queue, bound.pipeline, bytes)?;
            }
            Record::SetVertexBuffer { slot, buffer } => steps.push(Step::VertexBuffer {
                slot: *slot,
                buffer: gpu_buffer(st, device, queue, *buffer, wgpu::BufferUsages::VERTEX)?,
            }),
            Record::SetIndexBuffer { buffer, fmt } => steps.push(Step::IndexBuffer {
                buffer: gpu_buffer(st, device, queue, *buffer, wgpu::BufferUsages::INDEX)?,
                fmt: index_format(*fmt),
            }),
            Record::SetViewport {
                x,
                y,
                w,
                h,
                min_depth,
                max_depth,
            } => steps.push(Step::Viewport {
                x: *x,
                y: *y,
                w: *w,
                h: *h,
                min_depth: *min_depth,
                max_depth: *max_depth,
            }),
            Record::SetScissor { x, y, w, h } => steps.push(Step::Scissor {
                x: *x,
                y: *y,
                w: *w,
                h: *h,
            }),
            Record::Draw { .. }
            | Record::DrawIndexed { .. }
            | Record::DrawIndirect { .. }
            | Record::DrawIndexedIndirect { .. } => {
                if bound.pipeline < 0 {
                    return Err("draw before set_pipeline".into());
                }
                let groups = binds::resolve(
                    st,
                    device,
                    queue,
                    bound.pipeline,
                    &bound.explicit,
                    bound.list.as_ref(),
                    bound.uniform_offset,
                )?;
                if !groups.is_empty() {
                    steps.push(Step::BindGroups(groups));
                }
                steps.push(draw_step(st, device, queue, rec)?);
            }
            Record::BeginPass(_) | Record::EndPass => {
                return Err("nested render passes are not supported".into())
            }
            Record::WriteTimestamp { query_set, index } => {
                if !device
                    .features()
                    .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES)
                {
                    return Err(
                        "timestamp-query-inside-passes is not available on this device".into(),
                    );
                }
                if *index < 0 {
                    return Err("write_timestamp index must be >= 0".into());
                }
                steps.push(Step::WriteTimestamp {
                    query_set: super::super::queries::gpu_query_set(st, *query_set)?,
                    index: *index as u32,
                });
            }
        }
    }
    let query_set = if desc.query_set >= 0 {
        Some(super::super::queries::gpu_query_set(st, desc.query_set)?)
    } else {
        None
    };
    Ok(Pass {
        attachments,
        steps,
        query_set,
        ts_begin: desc.ts_begin,
        ts_end: desc.ts_end,
    })
}

fn draw_step(
    st: &mut GpuState,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rec: &Record,
) -> Result<Step, String> {
    Ok(match rec {
        Record::Draw {
            vertex_count,
            instance_count,
            first_vertex,
            first_instance,
        } => Step::Draw {
            vertices: *first_vertex..first_vertex + vertex_count,
            instances: *first_instance..first_instance + instance_count.max(&1),
        },
        Record::DrawIndexed {
            index_count,
            instance_count,
            first_index,
            base_vertex,
            first_instance,
        } => Step::DrawIndexed {
            indices: *first_index..first_index + index_count,
            base_vertex: *base_vertex,
            instances: *first_instance..first_instance + instance_count.max(&1),
        },
        Record::DrawIndirect { buffer, offset } => Step::DrawIndirect {
            buffer: gpu_buffer(st, device, queue, *buffer, wgpu::BufferUsages::INDIRECT)?,
            offset: *offset,
        },
        Record::DrawIndexedIndirect { buffer, offset } => Step::DrawIndexedIndirect {
            buffer: gpu_buffer(st, device, queue, *buffer, wgpu::BufferUsages::INDIRECT)?,
            offset: *offset,
        },
        _ => unreachable!("draw_step called with a non-draw record"),
    })
}

fn run_pass(encoder: &mut wgpu::CommandEncoder, pass: &Pass) {
    let colors: Vec<Option<wgpu::RenderPassColorAttachment<'_>>> = pass
        .attachments
        .colors
        .iter()
        .map(|c| {
            Some(wgpu::RenderPassColorAttachment {
                view: &c.view,
                resolve_target: c.resolve.as_ref(),
                ops: wgpu::Operations {
                    load: c.load,
                    store: c.store,
                },
            })
        })
        .collect();
    let writes = pass.query_set.as_ref().and_then(|qs| {
        super::super::queries::render_timestamp_writes(qs, pass.ts_begin, pass.ts_end)
    });
    let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("dream-rpass"),
        color_attachments: &colors,
        depth_stencil_attachment: pass.attachments.depth.as_ref().map(|d| {
            wgpu::RenderPassDepthStencilAttachment {
                view: &d.view,
                depth_ops: Some(wgpu::Operations {
                    load: d.load,
                    store: d.store,
                }),
                stencil_ops: d.stencil.map(|(load, store)| wgpu::Operations { load, store }),
            }
        }),
        timestamp_writes: writes,
        occlusion_query_set: None,
    });

    for step in &pass.steps {
        match step {
            Step::Pipeline(p) => rp.set_pipeline(p),
            Step::BindGroups(groups) => {
                for (group, bg, offsets) in groups {
                    rp.set_bind_group(*group, bg, offsets);
                }
            }
            Step::VertexBuffer { slot, buffer } => rp.set_vertex_buffer(*slot, buffer.slice(..)),
            Step::IndexBuffer { buffer, fmt } => rp.set_index_buffer(buffer.slice(..), *fmt),
            Step::Viewport {
                x,
                y,
                w,
                h,
                min_depth,
                max_depth,
            } => rp.set_viewport(*x, *y, *w, *h, *min_depth, *max_depth),
            Step::Scissor { x, y, w, h } => rp.set_scissor_rect(*x, *y, *w, *h),
            Step::Draw {
                vertices,
                instances,
            } => rp.draw(vertices.clone(), instances.clone()),
            Step::DrawIndexed {
                indices,
                base_vertex,
                instances,
            } => rp.draw_indexed(indices.clone(), *base_vertex, instances.clone()),
            Step::DrawIndirect { buffer, offset } => rp.draw_indirect(buffer, *offset),
            Step::DrawIndexedIndirect { buffer, offset } => {
                rp.draw_indexed_indirect(buffer, *offset)
            }
            Step::WriteTimestamp { query_set, index } => rp.write_timestamp(query_set, *index),
        }
    }
}

/// Total uniform ring space this frame's records will ask for.
///
/// The ring cannot grow once planning starts — a bind group holds the buffer it was built from —
/// so the requirement is totalled first. `set_pipeline` is tracked because the block size is the
/// pipeline's, and a `set_uniforms` naming an unknown pipeline is left for the planner to reject.
fn uniform_bytes_needed(st: &GpuState, device: &wgpu::Device, records: &[Record]) -> u64 {
    let mut pipeline = -1;
    let mut total = 0u64;
    for rec in records {
        match rec {
            Record::SetPipeline(id) => pipeline = *id,
            Record::SetUniforms(_) => {
                if let Some(rp) = st.render_pipes.get(&pipeline) {
                    if rp.uniform_size > 0 {
                        total += super::super::uniform_ring::UniformRing::stride_for(
                            rp.uniform_size,
                            device,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    total
}

/// Splits the flat record list into encoder-level ops (timestamp writes and render passes).
fn plan_all(
    st: &mut GpuState,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    records: Vec<Record>,
) -> Result<(Vec<FrameOp>, Vec<i32>), String> {
    let mut items = Vec::new();
    let mut resolve_ids = Vec::new();
    let mut iter = records.into_iter();
    while let Some(rec) = iter.next() {
        match rec {
            Record::WriteTimestamp { query_set, index } => {
                if !device
                    .features()
                    .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS)
                {
                    return Err(
                        "timestamp-query-inside-encoders is not available on this device".into(),
                    );
                }
                if index < 0 {
                    return Err("write_timestamp index must be >= 0".into());
                }
                let qs = super::super::queries::gpu_query_set(st, query_set)?;
                if !resolve_ids.contains(&query_set) {
                    resolve_ids.push(query_set);
                }
                items.push(FrameOp::WriteTimestamp {
                    query_set: qs,
                    index: index as u32,
                });
            }
            Record::BeginPass(desc) => {
                if desc.query_set >= 0 && !resolve_ids.contains(&desc.query_set) {
                    resolve_ids.push(desc.query_set);
                }
                let mut body = Vec::new();
                let mut closed = false;
                for inner in iter.by_ref() {
                    if matches!(inner, Record::EndPass) {
                        closed = true;
                        break;
                    }
                    if let Record::WriteTimestamp { query_set, .. } = &inner {
                        if *query_set >= 0 && !resolve_ids.contains(query_set) {
                            resolve_ids.push(*query_set);
                        }
                    }
                    body.push(inner);
                }
                if !closed {
                    return Err("render pass was never ended".into());
                }
                items.push(FrameOp::Pass(plan_pass(st, device, queue, &desc, &body)?));
            }
            _ => return Err("command outside of a render pass".into()),
        }
    }
    Ok((items, resolve_ids))
}

/// Replays a recorded stream: one command encoder, one queue submit.
pub fn submit(stream: &[u8]) -> Result<(), String> {
    let records = decode::parse(stream)?;
    if records.is_empty() {
        return Ok(());
    }

    let mut st = lock_state();
    if !st.ready {
        return Err("GPU not initialized".into());
    }
    let device = st.device.as_ref().unwrap().clone();
    let queue = st.queue.as_ref().unwrap().clone();

    // Every draw in this frame gets its own slice of the ring, so the whole frame's requirement is
    // reserved before the first bind group pins the buffer. A reservation that outgrows the ring
    // replaces it, stranding every cached group on the buffer it was built against.
    let needed = uniform_bytes_needed(&st, &device, &records);
    if st.uniform_ring.begin_frame(&device, needed) {
        st.render_bg_cache.clear();
    }

    let encode = super::super::profile::Span::start();
    let (items, resolve_ids) = plan_all(&mut st, &device, &queue, records)?;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("dream-frame"),
    });
    for item in &items {
        match item {
            FrameOp::Pass(pass) => run_pass(&mut encoder, pass),
            FrameOp::WriteTimestamp { query_set, index } => {
                encoder.write_timestamp(query_set, *index);
            }
        }
    }
    queue.submit(std::iter::once(encoder.finish()));
    // Render-pass timestamp writes are not visible to a resolve in the same command buffer
    // once the GPU is busy (the end stamp stays 0, so end < begin). Resolve after the pass
    // submission has finished.
    if !resolve_ids.is_empty() {
        let _ = device.poll(wgpu::Maintain::Wait);
        if let Some(err) = super::super::error::drain_uncaptured() {
            return Err(err);
        }
        let mut resolve_enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("dream-ts-resolve"),
        });
        for id in &resolve_ids {
            if let Some(entry) = st.query_sets.get(id) {
                super::super::queries::encode_resolve(&mut resolve_enc, entry)?;
            }
        }
        queue.submit(std::iter::once(resolve_enc.finish()));
    }
    if let Some(s) = encode {
        super::super::profile::note_encode(s.elapsed());
    }

    // Offscreen surface mirrors and render-target textures now hold fresh GPU content, so any
    // cached blit bind group and CPU mirror are stale.
    let touched: Vec<i32> = items
        .iter()
        .filter_map(|item| match item {
            FrameOp::Pass(p) => Some(p.attachments.textures.iter().copied()),
            FrameOp::WriteTimestamp { .. } => None,
        })
        .flatten()
        .collect();
    drop(items);
    for id in touched {
        if let Some(t) = st.textures.get_mut(&id) {
            t.dirty_cpu = false;
        }
        st.invalidate_blit_tex(id);
    }
    if let Some(err) = st.last_error.take() {
        return Err(err);
    }
    Ok(())
}
