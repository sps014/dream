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
    BindGroups(Vec<(u32, wgpu::BindGroup)>),
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
}

struct Pass {
    attachments: Attachments,
    steps: Vec<Step>,
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
    uniform_slot: Option<u32>,
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
                bound.uniform_slot =
                    Some(binds::alloc_uniform_slot(st, device, queue, bound.pipeline, bytes)?);
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
                    bound.uniform_slot,
                )?;
                if !groups.is_empty() {
                    steps.push(Step::BindGroups(groups));
                }
                steps.push(draw_step(st, device, queue, rec)?);
            }
            Record::BeginPass(_) | Record::EndPass => {
                return Err("nested render passes are not supported".into())
            }
        }
    }
    Ok(Pass { attachments, steps })
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
        timestamp_writes: None,
        occlusion_query_set: None,
    });

    for step in &pass.steps {
        match step {
            Step::Pipeline(p) => rp.set_pipeline(p),
            Step::BindGroups(groups) => {
                for (group, bg) in groups {
                    rp.set_bind_group(*group, bg, &[]);
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
        }
    }
}

/// Splits the flat record list into passes, planning each one as it goes.
fn plan_all(
    st: &mut GpuState,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    records: Vec<Record>,
) -> Result<Vec<Pass>, String> {
    let mut passes = Vec::new();
    let mut iter = records.into_iter().peekable();
    while let Some(rec) = iter.next() {
        let desc = match rec {
            Record::BeginPass(d) => d,
            _ => return Err("command outside of a render pass".into()),
        };
        let mut body = Vec::new();
        let mut closed = false;
        for inner in iter.by_ref() {
            if matches!(inner, Record::EndPass) {
                closed = true;
                break;
            }
            body.push(inner);
        }
        if !closed {
            return Err("render pass was never ended".into());
        }
        passes.push(plan_pass(st, device, queue, &desc, &body)?);
    }
    Ok(passes)
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

    // Uniform pool slots are handed out per submit, so every draw in this frame gets its own.
    for pipe in st.render_pipes.values_mut() {
        pipe.uniform_cursor = 0;
    }

    let encode = super::super::profile::Span::start();
    let passes = plan_all(&mut st, &device, &queue, records)?;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("dream-frame"),
    });
    for pass in &passes {
        run_pass(&mut encoder, pass);
    }
    queue.submit(std::iter::once(encoder.finish()));
    if let Some(s) = encode {
        super::super::profile::note_encode(s.elapsed());
    }

    // Offscreen surface mirrors and render-target textures now hold fresh GPU content, so any
    // cached blit bind group and CPU mirror are stale.
    let touched: Vec<i32> = passes
        .iter()
        .flat_map(|p| p.attachments.textures.iter().copied())
        .collect();
    drop(passes);
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
