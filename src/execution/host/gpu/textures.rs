//! Texture + sampler CPU/GPU resources.

use super::error::classify_err;
use super::state::{lock_state, SampEntry, TexEntry};

pub fn sampler_create(filter: i32) -> i32 {
    sampler_create_ex(filter, 0, 0)
}

pub fn sampler_create_ex(filter: i32, address: i32, mip_filter: i32) -> i32 {
    let mut st = lock_state();
    let id = st.alloc_id();
    st.samplers.insert(
        id,
        SampEntry {
            filter,
            address,
            mip_filter,
            gpu: None,
        },
    );
    id
}

pub fn texture_create_rgba8(width: i32, height: i32) -> i32 {
    texture_create(width, height, wgpu::TextureFormat::Rgba8Unorm, false, 1)
}

pub fn texture_create_depth(width: i32, height: i32) -> i32 {
    texture_create(width, height, wgpu::TextureFormat::Depth24Plus, true, 1)
}

pub fn texture_create_rgba16float(width: i32, height: i32) -> i32 {
    texture_create(width, height, wgpu::TextureFormat::Rgba16Float, false, 1)
}

pub fn texture_create_cube_rgba8(size: i32) -> i32 {
    texture_create(size, size, wgpu::TextureFormat::Rgba8Unorm, false, 6)
}

fn texture_create(
    width: i32,
    height: i32,
    format: wgpu::TextureFormat,
    depth: bool,
    layers: u32,
) -> i32 {
    let mut st = lock_state();
    let id = st.alloc_id();
    let w = width.max(1) as u32;
    let h = height.max(1) as u32;
    let bpp = if format == wgpu::TextureFormat::Rgba16Float {
        8
    } else if depth {
        0
    } else {
        4
    };
    let cpu = if bpp == 0 {
        Vec::new()
    } else {
        vec![0u8; (w * h * layers * bpp) as usize]
    };
    st.textures.insert(
        id,
        TexEntry {
            width: w,
            height: h,
            format,
            cpu,
            gpu: None,
            view: None,
            storage: false,
            depth,
            layers,
            dirty_cpu: true,
        },
    );
    id
}

pub fn texture_write_rgba(id: i32, pixels: Vec<u8>, x: i32, y: i32, w: i32, h: i32) -> i32 {
    let mut st = lock_state();
    let Some(tex) = st.textures.get_mut(&id) else {
        return classify_err(&format!("unknown texture {id}"));
    };
    if tex.depth {
        return classify_err("validation: cannot write rgba to depth texture");
    }
    let px = x.max(0) as u32;
    let py = y.max(0) as u32;
    let pw = w.max(0) as u32;
    let ph = h.max(0) as u32;
    let bpp = 4u32;
    for row in 0..ph {
        let dst = ((py + row) * tex.width + px) * bpp;
        let src = (row * pw * bpp) as usize;
        let n = (pw * bpp) as usize;
        let dst_i = dst as usize;
        if src + n <= pixels.len() && dst_i + n <= tex.cpu.len() {
            tex.cpu[dst_i..dst_i + n].copy_from_slice(&pixels[src..src + n]);
        }
    }
    tex.dirty_cpu = true;
    0
}

pub fn texture_read_rgba(id: i32) -> Vec<u8> {
    let st = lock_state();
    st.textures
        .get(&id)
        .map(|t| t.cpu.clone())
        .unwrap_or_default()
}

pub fn texture_copy_from_buffer(
    tex_id: i32,
    buf_id: i32,
    byte_offset: i32,
    _x: i32,
    _y: i32,
    w: i32,
    h: i32,
) {
    let mut st = lock_state();
    let Some(src) = st.buffers.get(&buf_id).map(|b| b.cpu.clone()) else {
        return;
    };
    let Some(tex) = st.textures.get_mut(&tex_id) else {
        return;
    };
    let off = byte_offset.max(0) as usize;
    let n = (w.max(0) * h.max(0) * 4) as usize;
    let end = (off + n).min(src.len());
    let take = end.saturating_sub(off).min(tex.cpu.len());
    if take > 0 {
        tex.cpu[..take].copy_from_slice(&src[off..off + take]);
        tex.dirty_cpu = true;
    }
}

pub fn texture_copy_to_buffer(
    tex_id: i32,
    buf_id: i32,
    byte_offset: i32,
    _x: i32,
    _y: i32,
    w: i32,
    h: i32,
) {
    let mut st = lock_state();
    let Some(tex_cpu) = st.textures.get(&tex_id).map(|t| t.cpu.clone()) else {
        return;
    };
    let dst = st.buffers.entry(buf_id).or_insert_with(|| super::state::BufEntry {
        cpu: Vec::new(),
        gpu: None,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
        created_usage: wgpu::BufferUsages::empty(),
        dirty_cpu: true,
    });
    let off = byte_offset.max(0) as usize;
    let n = (w.max(0) * h.max(0) * 4) as usize;
    let end = off + n;
    if end > dst.cpu.len() {
        dst.cpu.resize(end, 0);
    }
    let take = n.min(tex_cpu.len());
    if take > 0 {
        dst.cpu[off..off + take].copy_from_slice(&tex_cpu[..take]);
        dst.dirty_cpu = true;
    }
}
