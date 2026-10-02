//! Texture + sampler CPU/GPU resources.
//! Copy/write argument counts intentionally mirror the Dream host ABI shapes.

#![allow(clippy::too_many_arguments)]

use super::error::classify_err;
use super::formats;
use super::state::{lock_state, SampEntry, TexEntry};

#[allow(clippy::too_many_arguments)]
pub fn sampler_create(
    mag_filter: i32,
    min_filter: i32,
    mip_filter: i32,
    address_u: i32,
    address_v: i32,
    address_w: i32,
    lod_min: f32,
    lod_max: f32,
    compare: i32,
    max_anisotropy: i32,
) -> i32 {
    let mut st = lock_state();
    let id = st.alloc_id();
    st.samplers.insert(
        id,
        SampEntry {
            mag_filter,
            min_filter,
            mip_filter,
            address: [address_u, address_v, address_w],
            lod: (lod_min, lod_max.max(lod_min)),
            compare: (compare >= 0).then_some(compare),
            max_anisotropy: max_anisotropy.clamp(1, 16) as u16,
            gpu: None,
        },
    );
    id
}

/// Default view honoring the declared view dimension. A 6-layer 2D texture defaults to `D2Array`,
/// which cannot be bound to a `texture_cube<f32>` slot.
pub(crate) fn default_view(t: &TexEntry, gpu: &wgpu::Texture) -> wgpu::TextureView {
    gpu.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(t.view_dimension),
        ..Default::default()
    })
}

/// Allocates a texture from a `GpuTextureDesc`. Returns the id, or a negated `GpuError` code when
/// the format needs an unavailable device feature or the shape is impossible.
#[allow(clippy::too_many_arguments)]
pub fn texture_create(
    format: i32,
    dimension: i32,
    width: i32,
    height: i32,
    depth_or_layers: i32,
    mip_levels: i32,
    sample_count: i32,
    storage_access: i32,
    view_dimension: i32,
) -> i32 {
    let mut st = lock_state();
    let features = st
        .device
        .as_ref()
        .map(|d| d.features())
        // Before the device exists the format is accepted and re-checked at `ensure_texture`.
        .unwrap_or_else(wgpu::Features::all);
    let w = width.max(1) as u32;
    let h = height.max(1) as u32;
    let layers = depth_or_layers.max(1) as u32;
    let mips = mip_levels.max(1) as u32;
    let samples = sample_count.max(1) as u32;
    let plan = (|| {
        let resolved = formats::resolve(format, features)?;
        let dim = formats::dimension(dimension)?;
        let view = formats::view_dimension(view_dimension)?;
        let access = if storage_access < 0 {
            None
        } else if !resolved.spec.storage {
            return Err(format!(
                "validation: texture format {} cannot be a storage texture",
                resolved.spec.name
            ));
        } else {
            Some(formats::storage_access(storage_access)?)
        };
        validate_shape(&resolved, dim, view, w, h, layers, mips, samples)?;
        Ok((resolved, dim, view, access))
    })();
    let (resolved, dim, view, access) = match plan {
        Ok(p) => p,
        Err(e) => {
            let code = classify_err(&e);
            st.set_last_error(e);
            return -code;
        }
    };

    let id = st.alloc_id();
    // Only linearly copyable color formats get a CPU mirror; compressed and depth textures are
    // GPU-only, so their reads and writes are rejected rather than silently going through a
    // shadow buffer.
    let cpu = match resolved.bytes_per_texel() {
        Some(bpt) if !resolved.is_depth() => vec![0u8; (w * h * layers * bpt) as usize],
        _ => Vec::new(),
    };
    st.textures.insert(
        id,
        TexEntry {
            width: w,
            height: h,
            format: resolved,
            cpu,
            gpu: None,
            view: None,
            storage: access.is_some(),
            dimension: dim,
            layers,
            view_dimension: view,
            mip_levels: mips,
            sample_count: samples,
            dirty_cpu: true,
        },
    );
    id
}

/// Usage flags for a texture. Dream does not ask callers to declare usage, so every texture gets
/// everything its format and shape can legally support — WebGPU rejects flags a format cannot
/// honor, so the set has to be narrowed rather than always maximal.
pub(crate) fn texture_usage(t: &TexEntry) -> wgpu::TextureUsages {
    let mut usage = wgpu::TextureUsages::TEXTURE_BINDING;
    // Multisampled textures cannot be the source or destination of a copy.
    if t.sample_count <= 1 {
        usage |= wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST;
    }
    // Block-compressed and 1D/3D textures cannot be rendered into.
    if t.format.spec.block.is_none() && t.dimension == wgpu::TextureDimension::D2 {
        usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
    }
    if t.storage {
        usage |= wgpu::TextureUsages::STORAGE_BINDING;
    }
    usage
}

/// WebGPU shape rules that would otherwise fail deep inside `wgpu` with an opaque message.
#[allow(clippy::too_many_arguments)]
fn validate_shape(
    resolved: &formats::ResolvedFormat,
    dim: wgpu::TextureDimension,
    view: wgpu::TextureViewDimension,
    width: u32,
    height: u32,
    layers: u32,
    mips: u32,
    samples: u32,
) -> Result<(), String> {
    let cube = matches!(
        view,
        wgpu::TextureViewDimension::Cube | wgpu::TextureViewDimension::CubeArray
    );
    if cube {
        if !layers.is_multiple_of(6) {
            return Err(format!(
                "validation: a cube view needs a multiple of 6 layers, got {layers}"
            ));
        }
        if width != height {
            return Err(format!(
                "validation: cube faces must be square, got {width}x{height}"
            ));
        }
    }
    if view == wgpu::TextureViewDimension::D3 && dim != wgpu::TextureDimension::D3 {
        return Err("validation: a 3d view needs a 3d texture".to_string());
    }
    if dim == wgpu::TextureDimension::D3 && cube {
        return Err("validation: a 3d texture cannot have a cube view".to_string());
    }
    if dim == wgpu::TextureDimension::D1 && height > 1 {
        return Err(format!(
            "validation: a 1d texture must be 1 texel tall, got height {height}"
        ));
    }
    if samples > 1 {
        if mips > 1 {
            return Err("validation: a multisampled texture cannot have mip levels".to_string());
        }
        if layers > 1 {
            return Err("validation: a multisampled texture cannot be layered".to_string());
        }
        if dim != wgpu::TextureDimension::D2 {
            return Err("validation: only 2d textures can be multisampled".to_string());
        }
        if !matches!(samples, 2 | 4 | 8 | 16) {
            return Err(format!("validation: unsupported sample count {samples}"));
        }
    }
    let max_mips = 32 - (width.max(height).max(if dim == wgpu::TextureDimension::D3 {
        layers
    } else {
        1
    }))
    .leading_zeros();
    if mips > max_mips {
        return Err(format!(
            "validation: {width}x{height} allows at most {max_mips} mip levels, got {mips}"
        ));
    }
    if resolved.spec.block.is_some() && samples > 1 {
        return Err("validation: block-compressed textures cannot be multisampled".to_string());
    }
    Ok(())
}

/// Base-level CPU content changed: drop any mip chain so the next GPU create uses a single level.
/// Callers must re-run `generate_mipmaps` if filtered sampling is needed again.
pub(crate) fn note_cpu_content_change(tex: &mut TexEntry) {
    if tex.mip_levels > 1 {
        tex.mip_levels = 1;
        if let Some(gpu) = tex.gpu.take() {
            gpu.destroy();
        }
        tex.view = None;
    }
    tex.dirty_cpu = true;
}

pub fn texture_write_rgba(id: i32, pixels: Vec<u8>, x: i32, y: i32, w: i32, h: i32) -> i32 {
    let mut st = lock_state();
    let Some(tex) = st.textures.get_mut(&id) else {
        return classify_err(&format!("unknown texture {id}"));
    };
    let Some(bpp) = tex.format.bytes_per_texel().filter(|_| !tex.depth()) else {
        return classify_err(&format!(
            "validation: cannot write texels to a {} texture",
            tex.format.spec.name
        ));
    };
    let px = x.max(0) as u32;
    let py = y.max(0) as u32;
    let pw = w.max(0) as u32;
    let ph = h.max(0) as u32;
    for row in 0..ph {
        let dst = ((py + row) * tex.width + px) * bpp;
        let src = (row * pw * bpp) as usize;
        let n = (pw * bpp) as usize;
        let dst_i = dst as usize;
        if src + n <= pixels.len() && dst_i + n <= tex.cpu.len() {
            tex.cpu[dst_i..dst_i + n].copy_from_slice(&pixels[src..src + n]);
        }
    }
    note_cpu_content_change(tex);
    0
}

/// Reads back RGBA8 pixels, pulling from the GPU when it holds the authoritative copy (after a
/// render pass or compute write). Falls back to the CPU mirror when there is no GPU resource, so
/// headless paths still round-trip whatever was uploaded.
pub fn texture_read_rgba(id: i32) -> Vec<u8> {
    let mut st = lock_state();
    let Some(t) = st.textures.get(&id) else {
        return Vec::new();
    };
    let readable = t
        .format
        .bytes_per_texel()
        .filter(|_| !t.depth() && t.sample_count <= 1);
    let Some(bpp) = readable else {
        return t.cpu.clone();
    };
    if t.dirty_cpu || t.gpu.is_none() {
        return t.cpu.clone();
    }
    let (width, height) = (t.width.max(1), t.height.max(1));
    let texture = t.gpu.as_ref().unwrap().clone();
    let (Some(device), Some(queue)) = (st.device.clone(), st.queue.clone()) else {
        return t.cpu.clone();
    };

    // `copy_texture_to_buffer` requires 256-byte row alignment, so the staging rows are padded and
    // then compacted back to a tight `width * bpp` stride.
    let unpadded = (width * bpp) as usize;
    let padded = unpadded.div_ceil(256) * 256;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dream-tex-readback"),
        size: (padded * height as usize) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("dream-tex-readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded as u32),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::Maintain::Wait);
    let mapped = slice.get_mapped_range();
    let mut out = vec![0u8; unpadded * height as usize];
    for row in 0..height as usize {
        let src = row * padded;
        out[row * unpadded..(row + 1) * unpadded].copy_from_slice(&mapped[src..src + unpadded]);
    }
    drop(mapped);
    staging.unmap();

    if let Some(t) = st.textures.get_mut(&id) {
        t.cpu = out.clone();
    }
    out
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
        note_cpu_content_change(tex);
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

/// Snapshot of one side of a texture↔texture copy, taken before the mutable borrows begin.
struct CopyMeta {
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    bytes_per_texel: Option<u32>,
    depth: bool,
    gpu: Option<wgpu::Texture>,
}

/// GPU texture↔texture copy between same-format, linearly copyable textures. Falls back to a
/// CPU-shadow copy when the GPU resources are unavailable so headless/e2e paths still round-trip
/// pixels.
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
    let meta = |t: &TexEntry| CopyMeta {
        width: t.width,
        height: t.height,
        format: t.wgpu_format(),
        bytes_per_texel: t.format.bytes_per_texel(),
        depth: t.depth(),
        gpu: t.gpu.clone(),
    };
    let (Some(src_meta), Some(dst_meta)) = (
        st.textures.get(&src_id).map(meta),
        st.textures.get(&dst_id).map(meta),
    ) else {
        return;
    };
    if src_meta.depth || dst_meta.depth || src_meta.format != dst_meta.format {
        return;
    }
    let Some(bpp) = src_meta.bytes_per_texel else {
        return;
    };
    let sx = src_x.max(0) as u32;
    let sy = src_y.max(0) as u32;
    let dx = dst_x.max(0) as u32;
    let dy = dst_y.max(0) as u32;
    let mut w = width.max(0) as u32;
    let mut h = height.max(0) as u32;
    if sx + w > src_meta.width {
        w = src_meta.width.saturating_sub(sx);
    }
    if sy + h > src_meta.height {
        h = src_meta.height.saturating_sub(sy);
    }
    if dx + w > dst_meta.width {
        w = dst_meta.width.saturating_sub(dx);
    }
    if dy + h > dst_meta.height {
        h = dst_meta.height.saturating_sub(dy);
    }
    if w == 0 || h == 0 {
        return;
    }

    if let (Some(device), Some(queue), Some(src_gpu), Some(dst_gpu)) = (
        st.device.clone(),
        st.queue.clone(),
        src_meta.gpu.clone(),
        dst_meta.gpu.clone(),
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
        // Level 0 changed on a possibly multi-mip destination — collapse to a single-mip GPU
        // texture so filtered sampling never sees stale higher levels.
        collapse_dst_mips_after_level0_gpu_write(&mut st, dst_id, device, queue);
        return;
    }

    // CPU-shadow fallback.
    let src_cpu = st
        .textures
        .get(&src_id)
        .map(|t| t.cpu.clone())
        .unwrap_or_default();
    if let Some(dst) = st.textures.get_mut(&dst_id) {
        let stride_src = (src_meta.width * bpp) as usize;
        let stride_dst = (dst_meta.width * bpp) as usize;
        let row_bytes = (w * bpp) as usize;
        for row in 0..h {
            let so = ((sy + row) as usize) * stride_src + (sx * bpp) as usize;
            let dob = ((dy + row) as usize) * stride_dst + (dx * bpp) as usize;
            if so + row_bytes <= src_cpu.len() && dob + row_bytes <= dst.cpu.len() {
                dst.cpu[dob..dob + row_bytes].copy_from_slice(&src_cpu[so..so + row_bytes]);
            }
        }
        note_cpu_content_change(dst);
    }
    st.invalidate_blit_tex(dst_id);
}

/// After a GPU write to mip 0 only, rebuild `dst` as a single-mip texture (copying level 0)
/// when it previously had a longer chain.
fn collapse_dst_mips_after_level0_gpu_write(
    st: &mut super::state::GpuState,
    dst_id: i32,
    device: wgpu::Device,
    queue: wgpu::Queue,
) {
    let Some(dst) = st.textures.get_mut(&dst_id) else {
        return;
    };
    if dst.mip_levels <= 1 {
        dst.dirty_cpu = false;
        return;
    }
    let Some(old) = dst.gpu.take() else {
        note_cpu_content_change(dst);
        return;
    };
    let width = dst.width.max(1);
    let height = dst.height.max(1);
    let layers = dst.layers.max(1);
    let new_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("dream-tex-base"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: dst.sample_count.max(1),
        dimension: dst.dimension,
        format: dst.wgpu_format(),
        usage: texture_usage(dst),
        view_formats: &[],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("dream-collapse-mips"),
    });
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &old,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: &new_tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    old.destroy();
    dst.view = Some(default_view(dst, &new_tex));
    dst.gpu = Some(new_tex);
    dst.mip_levels = 1;
    dst.dirty_cpu = false;
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
    // The downsample below is a fixed 4-channel 8-bit box filter, so it only fits rgba8.
    if tex.wgpu_format() != wgpu::TextureFormat::Rgba8Unorm {
        return classify_err(&format!(
            "validation: mipmap generation requires an rgba8unorm texture, got {}",
            tex.format.spec.name
        ));
    }
    let width = tex.width.max(1);
    let height = tex.height.max(1);
    let layers = tex.layers.max(1);
    let format = tex.wgpu_format();
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
    tex.view = Some(default_view(tex, &gpu));
    tex.gpu = Some(gpu);
    tex.mip_levels = mip_count;
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
    let dst = st
        .buffers
        .entry(buf_id)
        .or_insert_with(|| super::state::BufEntry {
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

/// Largest edge length `from_image_bytes` will allocate. Matches the portable WebGPU
/// `maxTextureDimension2D` default, so a decode that succeeds here still creates on a weak device.
const MAX_IMAGE_EDGE: u32 = 8192;

/// Decodes PNG or JPEG bytes into tightly packed RGBA8. Rejects empty input, unknown codecs, and
/// images larger than [`MAX_IMAGE_EDGE`] on either side.
pub(crate) fn decode_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    if bytes.is_empty() {
        return Err("validation: image bytes are empty".into());
    }
    let img = image::load_from_memory(bytes)
        .map_err(|e| format!("unsupported: could not decode image ({e})"))?
        .into_rgba8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Err("validation: decoded image has zero size".into());
    }
    if w > MAX_IMAGE_EDGE || h > MAX_IMAGE_EDGE {
        return Err(format!(
            "unsupported: image {w}x{h} exceeds {MAX_IMAGE_EDGE} on an edge"
        ));
    }
    Ok((w, h, img.into_raw()))
}

/// Creates an RGBA8 2D texture from PNG or JPEG bytes. Returns `[id, width, height]`, or an empty
/// vec after recording a last-error when decode or allocation fails.
pub fn from_image_bytes(bytes: Vec<u8>) -> Vec<i32> {
    let (w, h, pixels) = match decode_rgba(&bytes) {
        Ok(decoded) => decoded,
        Err(e) => {
            let mut st = lock_state();
            st.set_last_error(e.clone());
            return vec![-classify_err(&e)];
        }
    };
    // `GpuTextureFormat.Rgba8Unorm` / `GpuTextureDimension.D2` / `GpuTextureViewDimension.D2`.
    let id = texture_create(2, 1, w as i32, h as i32, 1, 1, 1, -1, 1);
    if id < 0 {
        return vec![id];
    }
    let code = texture_write_rgba(id, pixels, 0, 0, w as i32, h as i32);
    if code != 0 {
        return vec![-code];
    }
    vec![id, w as i32, h as i32]
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use std::io::Cursor;

    fn png_solid(r: u8, g: u8, b: u8, a: u8) -> Vec<u8> {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_pixel(2, 1, Rgba([r, g, b, a]));
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    fn jpeg_solid(r: u8, g: u8, b: u8) -> Vec<u8> {
        let img: ImageBuffer<image::Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(1, 1, image::Rgb([r, g, b]));
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Jpeg)
            .unwrap();
        out
    }

    #[test]
    fn decode_png_keeps_rgba() {
        let (w, h, px) = decode_rgba(&png_solid(10, 20, 30, 40)).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(px, vec![10, 20, 30, 40, 10, 20, 30, 40]);
    }

    #[test]
    fn decode_jpeg_is_opaque() {
        let (w, h, px) = decode_rgba(&jpeg_solid(255, 0, 0)).unwrap();
        assert_eq!((w, h), (1, 1));
        assert_eq!(px[3], 255);
        assert!(px[0] > 200, "jpeg red channel drifted too far: {:?}", px);
    }

    #[test]
    fn decode_rejects_garbage() {
        let err = decode_rgba(&[0, 1, 2, 3]).unwrap_err();
        assert!(err.contains("unsupported"), "{}", err);
    }

    #[test]
    fn decode_rejects_empty() {
        let err = decode_rgba(&[]).unwrap_err();
        assert!(err.contains("empty"), "{}", err);
    }
}
