//! Texture + sampler CPU/GPU resources.
//! Copy/write argument counts intentionally mirror the Dream host ABI shapes.

#![allow(clippy::too_many_arguments)]

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

pub fn sampler_destroy(id: i32) {
    let mut st = lock_state();
    st.samplers.shift_remove(&id);
}

pub fn texture_destroy(id: i32) {
    let mut st = lock_state();
    st.invalidate_blit_tex(id);
    if let Some(entry) = st.textures.shift_remove(&id) {
        if let Some(gpu) = entry.gpu {
            gpu.destroy();
        }
    }
}

/// GPU texture↔texture copy (rgba8/rgba16f). Falls back to a CPU-shadow copy when the GPU
/// resources are unavailable so headless/e2e paths still round-trip pixels.
pub fn texture_copy(
    src_id: i32,
    dst_id: i32,
    src_x: i32,
    src_y: i32,
    dst_x: i32,
    dst_y: i32,
    width: i32,
    height: i32,
) {
    let mut st = lock_state();
    let (src_meta, dst_meta) = {
        let src = st.textures.get(&src_id);
        let dst = st.textures.get(&dst_id);
        match (src, dst) {
            (Some(s), Some(d)) => (
                (s.width, s.height, s.format, s.depth, s.gpu.clone()),
                (d.width, d.height, d.format, d.depth, d.gpu.clone()),
            ),
            _ => return,
        }
    };
    if src_meta.3 || dst_meta.3 {
        return;
    }
    if src_meta.2 != dst_meta.2 {
        return;
    }
    let bpp = if src_meta.2 == wgpu::TextureFormat::Rgba16Float {
        8u32
    } else {
        4
    };
    let sx = src_x.max(0) as u32;
    let sy = src_y.max(0) as u32;
    let dx = dst_x.max(0) as u32;
    let dy = dst_y.max(0) as u32;
    let mut w = width.max(0) as u32;
    let mut h = height.max(0) as u32;
    if sx + w > src_meta.0 { w = src_meta.0.saturating_sub(sx); }
    if sy + h > src_meta.1 { h = src_meta.1.saturating_sub(sy); }
    if dx + w > dst_meta.0 { w = dst_meta.0.saturating_sub(dx); }
    if dy + h > dst_meta.1 { h = dst_meta.1.saturating_sub(dy); }
    if w == 0 || h == 0 {
        return;
    }

    if let (Some(device), Some(queue), Some(src_gpu), Some(dst_gpu)) = (
        st.device.clone(),
        st.queue.clone(),
        src_meta.4.clone(),
        dst_meta.4.clone(),
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("dream-tex-copy"),
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &src_gpu,
                mip_level: 0,
                origin: wgpu::Origin3d { x: sx, y: sy, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &dst_gpu,
                mip_level: 0,
                origin: wgpu::Origin3d { x: dx, y: dy, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        st.invalidate_blit_tex(dst_id);
        if let Some(dst) = st.textures.get_mut(&dst_id) {
            dst.dirty_cpu = false;
        }
        return;
    }

    // CPU-shadow fallback.
    let src_cpu = st.textures.get(&src_id).map(|t| t.cpu.clone()).unwrap_or_default();
    if let Some(dst) = st.textures.get_mut(&dst_id) {
        let stride_src = (src_meta.0 * bpp) as usize;
        let stride_dst = (dst_meta.0 * bpp) as usize;
        let row_bytes = (w * bpp) as usize;
        for row in 0..h {
            let so = ((sy + row) as usize) * stride_src + (sx * bpp) as usize;
            let dob = ((dy + row) as usize) * stride_dst + (dx * bpp) as usize;
            if so + row_bytes <= src_cpu.len() && dob + row_bytes <= dst.cpu.len() {
                dst.cpu[dob..dob + row_bytes].copy_from_slice(&src_cpu[so..so + row_bytes]);
            }
        }
        dst.dirty_cpu = true;
    }
    st.invalidate_blit_tex(dst_id);
}

/// Builds a full mip chain for an rgba8unorm texture via CPU box-filter downsample and
/// recreates the GPU texture with `mip_level_count` so samplers can filter across levels.
/// Returns 0 on success, or a `GpuError` code.
pub fn texture_generate_mipmaps(id: i32) -> i32 {
    let mut st = lock_state();
    let device = st.device.clone();
    let queue = st.queue.clone();
    let Some(tex) = st.textures.get_mut(&id) else {
        return classify_err(&format!("unknown texture {id}"));
    };
    if tex.depth {
        return classify_err("validation: cannot generate mipmaps for depth texture");
    }
    if tex.format != wgpu::TextureFormat::Rgba8Unorm {
        return classify_err("validation: mipmap generation requires rgba8unorm texture");
    }
    let width = tex.width.max(1);
    let height = tex.height.max(1);
    let layers = tex.layers.max(1);
    let format = tex.format;
    let need = (width * height * 4) as usize;
    if tex.cpu.len() < need {
        tex.cpu.resize(need, 0);
    }
    let mut levels: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    levels.push((width, height, tex.cpu[..need].to_vec()));
    let mut prev_w = width;
    let mut prev_h = height;
    let mut prev = levels[0].2.clone();
    while prev_w > 1 || prev_h > 1 {
        let next_w = (prev_w / 2).max(1);
        let next_h = (prev_h / 2).max(1);
        let mut next = vec![0u8; (next_w * next_h * 4) as usize];
        for y in 0..next_h {
            for x in 0..next_w {
                let sx0 = (x * 2).min(prev_w - 1);
                let sx1 = (x * 2 + 1).min(prev_w - 1);
                let sy0 = (y * 2).min(prev_h - 1);
                let sy1 = (y * 2 + 1).min(prev_h - 1);
                for c in 0..4usize {
                    let a = prev[((sy0 * prev_w + sx0) * 4) as usize + c] as u32;
                    let b = prev[((sy0 * prev_w + sx1) * 4) as usize + c] as u32;
                    let cc = prev[((sy1 * prev_w + sx0) * 4) as usize + c] as u32;
                    let d = prev[((sy1 * prev_w + sx1) * 4) as usize + c] as u32;
                    next[((y * next_w + x) * 4) as usize + c] = ((a + b + cc + d + 2) / 4) as u8;
                }
            }
        }
        levels.push((next_w, next_h, next.clone()));
        prev = next;
        prev_w = next_w;
        prev_h = next_h;
        if levels.len() > 16 {
            break;
        }
    }
    let mip_count = levels.len() as u32;
    let (Some(device), Some(queue)) = (device, queue) else {
        tex.dirty_cpu = true;
        st.invalidate_blit_tex(id);
        return 0;
    };
    let gpu = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("dream-tex-mips"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: layers,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    for (level, (lw, lh, pixels)) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &gpu,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(lw * 4),
                rows_per_image: Some(*lh),
            },
            wgpu::Extent3d {
                width: *lw,
                height: *lh,
                depth_or_array_layers: 1,
            },
        );
    }
    tex.view = Some(gpu.create_view(&wgpu::TextureViewDescriptor::default()));
    tex.gpu = Some(gpu);
    tex.dirty_cpu = false;
    st.invalidate_blit_tex(id);
    0
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
