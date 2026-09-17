//! Parses a recorded command stream into neutral records.
//!
//! Wire format is documented once, in `crates/dream-stdlib/src/system/gpu/gpu_cmd.dream`. This
//! module only mirrors it; it resolves no GPU resources so the replay step can pre-scan a pass
//! (it needs the pipeline's sample count before it can build attachments).

pub const STREAM_VERSION: i32 = 1;

pub struct ColorAttachmentDesc {
    /// `0` the surface swapchain, `1` an offscreen texture.
    pub kind: i32,
    pub id: i32,
    pub resolve_id: i32,
    pub load: i32,
    pub store: i32,
    pub clear: [f32; 4],
}

pub struct PassDesc {
    pub colors: Vec<ColorAttachmentDesc>,
    /// `-1` none, `-2` the surface's own depth texture, otherwise a texture id.
    pub depth_id: i32,
    pub depth_load: i32,
    pub depth_store: i32,
    pub depth_clear: f32,
    pub stencil_load: i32,
    pub stencil_store: i32,
    pub stencil_clear: i32,
}

pub enum Record {
    BeginPass(PassDesc),
    EndPass,
    SetPipeline(i32),
    SetBindGroup {
        group: u32,
        id: i32,
    },
    SetBindList {
        buffers: Vec<i32>,
        textures: Vec<i32>,
        samplers: Vec<i32>,
    },
    SetUniforms(Vec<u8>),
    SetVertexBuffer {
        slot: u32,
        buffer: i32,
    },
    SetIndexBuffer {
        buffer: i32,
        fmt: i32,
    },
    SetViewport {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        min_depth: f32,
        max_depth: f32,
    },
    SetScissor {
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    },
    Draw {
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    },
    DrawIndexed {
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        base_vertex: i32,
        first_instance: u32,
    },
    DrawIndirect {
        buffer: i32,
        offset: u64,
    },
    DrawIndexedIndirect {
        buffer: i32,
        offset: u64,
    },
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn i32(&mut self) -> Result<i32, String> {
        if self.pos + 4 > self.buf.len() {
            return Err("command stream truncated".into());
        }
        let mut word = [0u8; 4];
        word.copy_from_slice(&self.buf[self.pos..self.pos + 4]);
        let v = i32::from_le_bytes(word);
        self.pos += 4;
        Ok(v)
    }

    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_bits(self.i32()? as u32))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(self.i32()?.max(0) as u32)
    }

    fn i32_array(&mut self) -> Result<Vec<i32>, String> {
        let n = self.i32()?.max(0) as usize;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(self.i32()?);
        }
        Ok(out)
    }

    /// Length-prefixed payload, zero-padded to a 4-byte boundary by the writer.
    fn blob(&mut self) -> Result<Vec<u8>, String> {
        let n = self.i32()?.max(0) as usize;
        let padded = n.next_multiple_of(4);
        if self.pos + padded > self.buf.len() {
            return Err("command stream blob truncated".into());
        }
        let out = self.buf[self.pos..self.pos + n].to_vec();
        self.pos += padded;
        Ok(out)
    }
}

pub fn parse(stream: &[u8]) -> Result<Vec<Record>, String> {
    let mut r = Reader {
        buf: stream,
        pos: 0,
    };
    let version = r.i32()?;
    if version != STREAM_VERSION {
        return Err(format!(
            "unsupported command stream version {version} (host understands {STREAM_VERSION})"
        ));
    }
    let mut out = Vec::new();
    while r.pos < stream.len() {
        let op = r.i32()?;
        out.push(match op {
            1 => {
                let color_count = r.i32()?.max(0) as usize;
                let depth_id = r.i32()?;
                let depth_load = r.i32()?;
                let depth_store = r.i32()?;
                let depth_clear = r.f32()?;
                let stencil_load = r.i32()?;
                let stencil_store = r.i32()?;
                let stencil_clear = r.i32()?;
                let mut colors = Vec::with_capacity(color_count);
                for _ in 0..color_count {
                    colors.push(ColorAttachmentDesc {
                        kind: r.i32()?,
                        id: r.i32()?,
                        resolve_id: r.i32()?,
                        load: r.i32()?,
                        store: r.i32()?,
                        clear: [r.f32()?, r.f32()?, r.f32()?, r.f32()?],
                    });
                }
                Record::BeginPass(PassDesc {
                    colors,
                    depth_id,
                    depth_load,
                    depth_store,
                    depth_clear,
                    stencil_load,
                    stencil_store,
                    stencil_clear,
                })
            }
            2 => Record::EndPass,
            3 => Record::SetPipeline(r.i32()?),
            4 => Record::SetBindGroup {
                group: r.u32()?,
                id: r.i32()?,
            },
            5 => Record::SetUniforms(r.blob()?),
            6 => Record::SetVertexBuffer {
                slot: r.u32()?,
                buffer: r.i32()?,
            },
            7 => Record::SetIndexBuffer {
                buffer: r.i32()?,
                fmt: r.i32()?,
            },
            8 => Record::SetViewport {
                x: r.f32()?,
                y: r.f32()?,
                w: r.f32()?,
                h: r.f32()?,
                min_depth: r.f32()?,
                max_depth: r.f32()?,
            },
            9 => Record::SetScissor {
                x: r.u32()?,
                y: r.u32()?,
                w: r.u32()?,
                h: r.u32()?,
            },
            10 => Record::Draw {
                vertex_count: r.u32()?,
                instance_count: r.u32()?,
                first_vertex: r.u32()?,
                first_instance: r.u32()?,
            },
            11 => Record::DrawIndexed {
                index_count: r.u32()?,
                instance_count: r.u32()?,
                first_index: r.u32()?,
                base_vertex: r.i32()?,
                first_instance: r.u32()?,
            },
            12 => Record::DrawIndirect {
                buffer: r.i32()?,
                offset: r.u32()? as u64,
            },
            13 => Record::DrawIndexedIndirect {
                buffer: r.i32()?,
                offset: r.u32()? as u64,
            },
            14 => Record::SetBindList {
                buffers: r.i32_array()?,
                textures: r.i32_array()?,
                samplers: r.i32_array()?,
            },
            other => return Err(format!("unknown command stream opcode {other}")),
        });
    }
    Ok(out)
}
