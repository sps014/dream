//! Buffer CPU staging + GPU upload/download.

use super::state::{lock_state, BufEntry};
use wgpu::util::DeviceExt;

pub fn alloc_bytes(n: i32) -> i32 {
    let mut st = lock_state();
    let id = st.alloc_id();
    st.buffers.insert(
        id,
        BufEntry {
            cpu: vec![0u8; n.max(0) as usize],
            gpu: None,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::UNIFORM,
            dirty_cpu: true,
        },
    );
    id
}

pub fn alloc_vertex_bytes(n: i32) -> i32 {
    let mut st = lock_state();
    let id = st.alloc_id();
    st.buffers.insert(
        id,
        BufEntry {
            cpu: vec![0u8; n.max(0) as usize],
            gpu: None,
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::INDEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::STORAGE,
            dirty_cpu: true,
        },
    );
    id
}

pub fn write_bytes(id: i32, bytes: Vec<u8>) -> Result<(), String> {
    let mut st = lock_state();
    let buf = st
        .buffers
        .get_mut(&id)
        .ok_or_else(|| format!("unknown GpuBuffer {id}"))?;
    buf.cpu = bytes;
    buf.dirty_cpu = true;
    Ok(())
}

pub fn write_bytes_at(id: i32, byte_offset: i32, bytes: Vec<u8>) -> Result<(), String> {
    let mut st = lock_state();
    let buf = st
        .buffers
        .get_mut(&id)
        .ok_or_else(|| format!("unknown GpuBuffer {id}"))?;
    let off = byte_offset.max(0) as usize;
    let end = off + bytes.len();
    if end > buf.cpu.len() {
        buf.cpu.resize(end, 0);
    }
    buf.cpu[off..end].copy_from_slice(&bytes);
    buf.dirty_cpu = true;
    Ok(())
}

pub fn read_bytes(id: i32, n: i32) -> Result<Vec<u8>, String> {
    sync_gpu_to_cpu(id)?;
    let st = lock_state();
    let buf = st
        .buffers
        .get(&id)
        .ok_or_else(|| format!("unknown GpuBuffer {id}"))?;
    let take = n.max(0) as usize;
    let mut out = buf.cpu.clone();
    out.resize(take, 0);
    if out.len() > take {
        out.truncate(take);
    }
    Ok(out)
}

pub fn read_bytes_at(id: i32, byte_offset: i32, n: i32) -> Result<Vec<u8>, String> {
    sync_gpu_to_cpu(id)?;
    let st = lock_state();
    let buf = st
        .buffers
        .get(&id)
        .ok_or_else(|| format!("unknown GpuBuffer {id}"))?;
    let off = byte_offset.max(0) as usize;
    let take = n.max(0) as usize;
    let end = (off + take).min(buf.cpu.len());
    let mut out = if off < buf.cpu.len() {
        buf.cpu[off..end].to_vec()
    } else {
        Vec::new()
    };
    out.resize(take, 0);
    Ok(out)
}

pub fn copy(src_id: i32, dst_id: i32, src_off: i32, dst_off: i32, size: i32) {
    let mut st = lock_state();
    let n = size.max(0) as usize;
    let so = src_off.max(0) as usize;
    let d_off = dst_off.max(0) as usize;
    let src = st.buffers.get(&src_id).map(|b| b.cpu.clone()).unwrap_or_default();
    let dst = st.buffers.entry(dst_id).or_insert_with(|| BufEntry {
        cpu: Vec::new(),
        gpu: None,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
        dirty_cpu: true,
    });
    let end = d_off + n;
    if end > dst.cpu.len() {
        dst.cpu.resize(end, 0);
    }
    let take = n.min(src.len().saturating_sub(so));
    if take > 0 {
        dst.cpu[d_off..d_off + take].copy_from_slice(&src[so..so + take]);
    }
    dst.dirty_cpu = true;
}

/// Ensure a GPU buffer exists and CPU contents are uploaded.
pub fn ensure_gpu_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    entry: &mut BufEntry,
    extra: wgpu::BufferUsages,
) -> Result<(), String> {
    entry.usage |= extra;
    let size = entry.cpu.len().max(4) as u64;
    let size = (size + 3) & !3; // wgpu storage buffers prefer 4-byte alignment
    if entry.gpu.as_ref().is_some_and(|b| b.size() >= size) && !entry.dirty_cpu {
        return Ok(());
    }
    if entry.cpu.len() < size as usize {
        entry.cpu.resize(size as usize, 0);
    }
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("dream-buf"),
        contents: &entry.cpu[..size as usize],
        usage: entry.usage
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
    });
    entry.gpu = Some(buf);
    entry.dirty_cpu = false;
    let _ = queue;
    Ok(())
}

fn sync_gpu_to_cpu(id: i32) -> Result<(), String> {
    let mut st = lock_state();
    let device = st
        .device
        .as_ref()
        .ok_or_else(|| "GPU device not initialized".to_string())?
        .clone();
    let queue = st
        .queue
        .as_ref()
        .ok_or_else(|| "GPU queue not initialized".to_string())?
        .clone();
    let entry = st
        .buffers
        .get_mut(&id)
        .ok_or_else(|| format!("unknown GpuBuffer {id}"))?;
    // CPU write is newer than last GPU download — skip readback.
    if entry.dirty_cpu {
        return Ok(());
    }
    let Some(gpu) = entry.gpu.as_ref() else {
        return Ok(());
    };
    let size = entry.cpu.len().max(4) as u64;
    let size = (size + 3) & !3;
    if entry.cpu.len() < size as usize {
        entry.cpu.resize(size as usize, 0);
    }
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dream-readback"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("dream-readback-enc"),
    });
    encoder.copy_buffer_to_buffer(gpu, 0, &staging, 0, size);
    queue.submit(Some(encoder.finish()));
    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv()
        .map_err(|_| "readback channel closed".to_string())?
        .map_err(|e| format!("map failed: {e}"))?;
    {
        let data = slice.get_mapped_range();
        entry.cpu[..size as usize].copy_from_slice(&data);
    }
    staging.unmap();
    Ok(())
}
