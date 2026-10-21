//! `gpuSubmit`: validates an encoded command stream against host state, records it through the
//! frame/encoder glue, and resolves the guest future once the GPU finished it.

use crate::bundle::ArgKind;
use crate::commands::{Op, decode};
use crate::error::{self, Code, HostError, HostResult, check};
use crate::ffi;
use crate::state::{self, Kind, PipelineEntry, PipelineKind, State};
use crate::waiter;
use std::ffi::c_void;

pub fn submit(future: usize, device: i32, words: &[i32], data: &[u8]) {
    if let Err(e) = submit_now(future, device, words, data) {
        error::record(&e);
        waiter::complete(future, 1);
    }
}

fn submit_now(future: usize, device: i32, words: &[i32], data: &[u8]) -> HostResult<()> {
    let ops = decode(words).map_err(HostError::invalid)?;
    let mut state = state::lock();
    let core = state.device(device)?;
    let mut encoder = std::ptr::null_mut();
    check(unsafe { ffi::dgpu_encoder_begin(core.raw(), &mut encoder) })?;
    let mut used = Vec::new();
    let recorded = Recorder {
        state: &state,
        device,
        encoder,
        data,
        used: &mut used,
        pass: Pass::None,
        pipeline: None,
    }
    .run(&ops);
    if let Err(e) = recorded {
        unsafe { ffi::dgpu_encoder_abort(encoder) };
        return Err(e);
    }
    let mut fence = 0;
    check(unsafe { ffi::dgpu_encoder_submit(encoder, &mut fence) })?;
    for handle in used {
        state.touch(handle, fence);
    }
    core.push(
        fence,
        future,
        Box::new(|_, waited| match waited {
            Ok(()) => 0,
            Err(e) => {
                error::record(&e);
                1
            }
        }),
    );
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    None,
    Render,
    Compute,
}

struct Recorder<'a> {
    state: &'a State,
    device: i32,
    encoder: *mut ffi::DgpuEncoder,
    data: &'a [u8],
    used: &'a mut Vec<i32>,
    pass: Pass,
    pipeline: Option<&'a PipelineEntry>,
}

fn validation(detail: impl Into<String>) -> HostError {
    HostError::new(Code::Validation, detail)
}

fn count(value: i32, what: &str) -> HostResult<u32> {
    u32::try_from(value).map_err(|_| validation(format!("{what} must not be negative")))
}

impl<'a> Recorder<'a> {
    fn buffer(&mut self, handle: i32, what: &str) -> HostResult<(*mut c_void, u64)> {
        let entry = self.state.owned(handle, self.device, what)?;
        let Kind::Buffer(buffer) = &entry.kind else {
            return Err(HostError::invalid(format!(
                "the {what} is a {}",
                entry.kind.name()
            )));
        };
        self.used.push(handle);
        Ok((buffer.obj.raw(), buffer.size))
    }

    fn texture(&mut self, handle: i32, what: &str) -> HostResult<*mut c_void> {
        let entry = self.state.owned(handle, self.device, what)?;
        let Kind::Texture(texture) = &entry.kind else {
            return Err(HostError::invalid(format!(
                "the {what} is a {}",
                entry.kind.name()
            )));
        };
        self.used.push(handle);
        Ok(texture.obj.raw())
    }

    fn sampler(&mut self, handle: i32) -> HostResult<*mut c_void> {
        let entry = self.state.owned(handle, self.device, "sampler")?;
        let Kind::Sampler(sampler) = &entry.kind else {
            return Err(HostError::invalid(format!(
                "the sampler is a {}",
                entry.kind.name()
            )));
        };
        self.used.push(handle);
        Ok(sampler.raw())
    }

    fn acceleration_structure(&mut self, handle: i32) -> HostResult<*mut c_void> {
        let entry = self
            .state
            .owned(handle, self.device, "acceleration structure")?;
        let Kind::AccelerationStructure(structure) = &entry.kind else {
            return Err(HostError::invalid(format!(
                "the acceleration structure is a {}",
                entry.kind.name()
            )));
        };
        if !structure.top_level {
            return Err(validation(
                "shaders trace rays against top-level acceleration structures",
            ));
        }
        self.used.push(handle);
        Ok(structure.obj.raw())
    }

    fn in_pass(&self, wanted: Pass, what: &str) -> HostResult<()> {
        if self.pass != wanted {
            let pass = if wanted == Pass::Render {
                "render"
            } else {
                "compute"
            };
            return Err(validation(format!("{what} needs an open {pass} pass")));
        }
        Ok(())
    }

    fn run(mut self, ops: &[Op]) -> HostResult<()> {
        for op in ops {
            self.op(op)?;
        }
        if self.pass != Pass::None {
            return Err(validation("a pass was not ended before submit"));
        }
        Ok(())
    }

    fn op(&mut self, op: &Op) -> HostResult<()> {
        let e = self.encoder;
        match *op {
            Op::BeginRenderPass {
                color,
                clear,
                depth,
                depth_clear,
            } => {
                if self.pass != Pass::None {
                    return Err(validation("a pass was begun before the previous one ended"));
                }
                let color = self.texture(color, "color target")?;
                let depth = if depth == 0 {
                    std::ptr::null_mut()
                } else {
                    self.texture(depth, "depth target")?
                };
                let rgba = clear.unwrap_or([0.0; 4]);
                check(unsafe {
                    ffi::dgpu_enc_begin_render_pass(
                        e,
                        color,
                        clear.is_some() as i32,
                        rgba.as_ptr(),
                        depth,
                        depth_clear.is_some() as i32,
                        depth_clear.unwrap_or(1.0),
                    )
                })?;
                self.pass = Pass::Render;
                self.pipeline = None;
            }
            Op::BeginComputePass => {
                if self.pass != Pass::None {
                    return Err(validation("a pass was begun before the previous one ended"));
                }
                check(unsafe { ffi::dgpu_enc_begin_compute_pass(e) })?;
                self.pass = Pass::Compute;
                self.pipeline = None;
            }
            Op::EndPass => {
                if self.pass == Pass::None {
                    return Err(validation("EndPass without an open pass"));
                }
                check(unsafe { ffi::dgpu_enc_end_pass(e) })?;
                self.pass = Pass::None;
                self.pipeline = None;
            }
            Op::SetPipeline(handle) => {
                let state = self.state;
                let entry = state.owned(handle, self.device, "pipeline")?;
                let Kind::Pipeline(pipeline) = &entry.kind else {
                    return Err(HostError::invalid(format!(
                        "the pipeline is a {}",
                        entry.kind.name()
                    )));
                };
                let pass = if pipeline.kind == PipelineKind::Render {
                    Pass::Render
                } else {
                    Pass::Compute
                };
                self.in_pass(pass, "the pipeline")?;
                check(unsafe { ffi::dgpu_enc_set_pipeline(e, pipeline.pipeline.raw()) })?;
                self.used.push(handle);
                self.pipeline = Some(pipeline);
            }
            Op::Bind {
                stage,
                index,
                kind,
                a,
                b,
            } => self.bind(stage, index, kind, a, b)?,
            Op::Dispatch(groups) => {
                self.in_pass(Pass::Compute, "dispatch")?;
                if self
                    .pipeline
                    .is_none_or(|p| p.kind != PipelineKind::Compute)
                {
                    return Err(validation("dispatch needs a compute pipeline"));
                }
                let [x, y, z] = [
                    count(groups[0], "dispatch size")?,
                    count(groups[1], "dispatch size")?,
                    count(groups[2], "dispatch size")?,
                ];
                check(unsafe { ffi::dgpu_enc_dispatch(e, x, y, z) })?;
            }
            Op::SetVertexBuffer {
                slot,
                buffer,
                offset,
            } => {
                self.in_pass(Pass::Render, "set_vertex_buffer")?;
                let slot = count(slot, "vertex buffer slot").and_then(|s| {
                    if s < 16 {
                        Ok(s)
                    } else {
                        Err(validation("vertex buffer slots stop at 15"))
                    }
                })?;
                let (raw, size) = self.buffer(buffer, "vertex buffer")?;
                let offset = count(offset, "vertex buffer offset")? as u64;
                if offset > size {
                    return Err(HostError::invalid(
                        "vertex buffer offset lies outside the buffer",
                    ));
                }
                check(unsafe { ffi::dgpu_enc_set_vertex_buffer(e, slot, raw, offset) })?;
            }
            Op::SetIndexBuffer {
                buffer,
                format,
                offset,
            } => {
                self.in_pass(Pass::Render, "set_index_buffer")?;
                if !(0..=1).contains(&format) {
                    return Err(HostError::invalid("unknown index format"));
                }
                let (raw, size) = self.buffer(buffer, "index buffer")?;
                let offset = count(offset, "index buffer offset")? as u64;
                if offset > size {
                    return Err(HostError::invalid(
                        "index buffer offset lies outside the buffer",
                    ));
                }
                check(unsafe { ffi::dgpu_enc_set_index_buffer(e, raw, format as u32, offset) })?;
            }
            Op::SetViewport(v) => {
                self.in_pass(Pass::Render, "set_viewport")?;
                if v.iter().any(|f| !f.is_finite()) || v[2] < 0.0 || v[3] < 0.0 {
                    return Err(validation(
                        "viewport values must be finite and sizes non-negative",
                    ));
                }
                check(unsafe { ffi::dgpu_enc_set_viewport(e, v.as_ptr()) })?;
            }
            Op::SetScissor(s) => {
                self.in_pass(Pass::Render, "set_scissor")?;
                for value in s {
                    count(value, "scissor rectangle values")?;
                }
                if s[0].checked_add(s[2]).is_none() || s[1].checked_add(s[3]).is_none() {
                    return Err(validation("scissor rectangle overflows"));
                }
                check(unsafe { ffi::dgpu_enc_set_scissor(e, s[0], s[1], s[2], s[3]) })?;
            }
            Op::Draw(d) => {
                self.in_pass(Pass::Render, "draw")?;
                if self.pipeline.is_none() {
                    return Err(validation("draw needs a render pipeline"));
                }
                let d = [
                    count(d[0], "draw")?,
                    count(d[1], "draw")?,
                    count(d[2], "draw")?,
                    count(d[3], "draw")?,
                ];
                check(unsafe { ffi::dgpu_enc_draw(e, d[0], d[1], d[2], d[3]) })?;
            }
            Op::DrawIndexed(d) => {
                self.in_pass(Pass::Render, "draw_indexed")?;
                if self.pipeline.is_none() {
                    return Err(validation("draw_indexed needs a render pipeline"));
                }
                check(unsafe {
                    ffi::dgpu_enc_draw_indexed(
                        e,
                        count(d[0], "draw_indexed")?,
                        count(d[1], "draw_indexed")?,
                        count(d[2], "draw_indexed")?,
                        d[3],
                        count(d[4], "draw_indexed")?,
                    )
                })?;
            }
            Op::CopyBuffer {
                source,
                source_offset,
                destination,
                destination_offset,
                size,
            } => {
                if self.pass != Pass::None {
                    return Err(validation("copy_buffer was recorded inside a pass"));
                }
                let (src, src_size) = self.buffer(source, "copy source")?;
                let (dst, dst_size) = self.buffer(destination, "copy destination")?;
                let size = count(size, "copy size")? as u64;
                let src_offset = count(source_offset, "copy offset")? as u64;
                let dst_offset = count(destination_offset, "copy offset")? as u64;
                if src_offset + size > src_size || dst_offset + size > dst_size {
                    return Err(HostError::invalid("copy range lies outside a buffer"));
                }
                check(unsafe {
                    ffi::dgpu_enc_copy_buffer(e, dst, dst_offset, src, src_offset, size)
                })?;
            }
            Op::TraceRays(size) => {
                self.in_pass(Pass::Compute, "trace_rays")?;
                if self
                    .pipeline
                    .is_none_or(|p| p.kind != PipelineKind::RayTracing)
                {
                    return Err(validation("trace_rays needs a ray tracing pipeline"));
                }
                let [width, height, depth] = [
                    count(size[0], "trace size")?,
                    count(size[1], "trace size")?,
                    count(size[2], "trace size")?,
                ];
                check(unsafe { ffi::dgpu_enc_trace_rays(e, width, height, depth) })?;
            }
        }
        Ok(())
    }

    fn bind(&mut self, stage: i32, index: i32, kind: i32, a: i32, b: i32) -> HostResult<()> {
        let pipeline = self
            .pipeline
            .ok_or_else(|| validation("bind needs a pipeline set first"))?;
        // GpuStage: Compute 0, Vertex 1, Fragment 2, RayGeneration 3. Only these stages take
        // parameters; a ray tracing program lists its ray generation shader first.
        let entry = match (pipeline.kind, stage) {
            (PipelineKind::Compute, 0) => 0,
            (PipelineKind::Render, 1) => 0,
            (PipelineKind::Render, 2) => 1,
            (PipelineKind::RayTracing, 3) => 0,
            _ => {
                return Err(validation(format!(
                    "the pipeline has no shader for stage {stage}"
                )));
            }
        };
        let param = usize::try_from(index)
            .ok()
            .and_then(|i| pipeline.stages[entry].get(i))
            .ok_or_else(|| HostError::invalid(format!("the shader has no parameter {index}")))?;
        let kind = ArgKind::from_i32(kind)
            .ok_or_else(|| HostError::invalid(format!("unknown argument kind {kind}")))?;
        if kind != param.kind {
            return Err(validation(format!(
                "parameter `{}` takes {:?}, not {kind:?}",
                param.name, param.kind
            )));
        }
        let (resource, bytes): (*mut c_void, &[u8]) = match kind {
            ArgKind::Value => {
                let start = count(a, "value offset")? as usize;
                let len = count(b, "value size")? as usize;
                let bytes = self
                    .data
                    .get(start..start.saturating_add(len))
                    .ok_or_else(|| HostError::invalid("value bytes lie outside the submit data"))?;
                if param.size > 0 && len as u32 > param.size {
                    return Err(validation(format!(
                        "parameter `{}` holds {} bytes, got {len}",
                        param.name, param.size
                    )));
                }
                (std::ptr::null_mut(), bytes)
            }
            ArgKind::Buffer | ArgKind::MlpWeights => (self.buffer(a, "buffer")?.0, &[]),
            ArgKind::Texture | ArgKind::StorageTexture => (self.texture(a, "texture")?, &[]),
            ArgKind::Sampler => (self.sampler(a)?, &[]),
            ArgKind::AccelerationStructure => (self.acceleration_structure(a)?, &[]),
        };
        check(unsafe {
            ffi::dgpu_enc_bind(
                self.encoder,
                entry as u32,
                index as u32,
                resource,
                bytes.as_ptr().cast(),
                bytes.len() as u32,
            )
        })
    }
}
