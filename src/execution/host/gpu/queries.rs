//! Timestamp query sets (`timestamp-query`) + resolve/readback.

use super::state::{lock_state, QuerySetEntry, ERR_UNSUPPORTED};
use std::sync::mpsc;

fn idx(v: i32) -> Option<u32> {
    if v < 0 {
        None
    } else {
        Some(v as u32)
    }
}

pub fn create_timestamps(count: i32) -> i32 {
    let mut st = lock_state();
    if !st.ready {
        return -super::state::ERR_UNAVAILABLE;
    }
    let device = st.device.as_ref().unwrap();
    if !device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
    {
        st.set_last_error("timestamp-query is not available on this device".into());
        return -ERR_UNSUPPORTED;
    }
    let n = count.max(1) as u32;
    let qs = device.create_query_set(&wgpu::QuerySetDescriptor {
        label: Some("dream-timestamps"),
        ty: wgpu::QueryType::Timestamp,
        count: n,
    });
    let bytes = u64::from(n) * 8;
    let resolve = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dream-ts-resolve"),
        size: bytes,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dream-ts-readback"),
        size: bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let id = st.alloc_id();
    st.query_sets.insert(
        id,
        QuerySetEntry {
            gpu: Some(qs),
            count: n,
            resolve: Some(resolve),
            readback: Some(readback),
            last_ns: vec![0; n as usize],
        },
    );
    id
}

pub fn destroy(id: i32) {
    let mut st = lock_state();
    st.query_sets.shift_remove(&id);
}

pub fn period() -> f32 {
    let st = lock_state();
    st.queue
        .as_ref()
        .map(|q| q.get_timestamp_period())
        .unwrap_or(1.0)
}

pub fn compute_timestamp_writes<'a>(
    qs: &'a wgpu::QuerySet,
    begin: i32,
    end: i32,
) -> Option<wgpu::ComputePassTimestampWrites<'a>> {
    if begin < 0 && end < 0 {
        return None;
    }
    Some(wgpu::ComputePassTimestampWrites {
        query_set: qs,
        beginning_of_pass_write_index: idx(begin),
        end_of_pass_write_index: idx(end),
    })
}

/// Resolve the whole set into its staging buffers on `encoder`.
pub fn encode_resolve(
    encoder: &mut wgpu::CommandEncoder,
    entry: &QuerySetEntry,
) -> Result<(), String> {
    let qs = entry
        .gpu
        .as_ref()
        .ok_or_else(|| "query set has no GPU object".to_string())?;
    let resolve = entry
        .resolve
        .as_ref()
        .ok_or_else(|| "query set has no resolve buffer".to_string())?;
    let readback = entry
        .readback
        .as_ref()
        .ok_or_else(|| "query set has no readback buffer".to_string())?;
    let bytes = u64::from(entry.count) * 8;
    encoder.resolve_query_set(qs, 0..entry.count, resolve, 0);
    encoder.copy_buffer_to_buffer(resolve, 0, readback, 0, bytes);
    Ok(())
}

pub fn map_readback(id: i32) -> Result<Vec<i64>, String> {
    let (readback, count, period) = {
        let st = lock_state();
        let entry = st
            .query_sets
            .get(&id)
            .ok_or_else(|| format!("unknown query set {id}"))?;
        let readback = entry
            .readback
            .as_ref()
            .ok_or_else(|| "query set has no readback buffer".to_string())?
            .clone();
        let period = st
            .queue
            .as_ref()
            .map(|q| q.get_timestamp_period())
            .unwrap_or(1.0);
        (readback, entry.count, period)
    };
    let device = {
        let st = lock_state();
        st.device
            .as_ref()
            .ok_or_else(|| "GPU not initialized".to_string())?
            .clone()
    };
    let bytes = u64::from(count) * 8;
    let slice = readback.slice(0..bytes);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv()
        .map_err(|_| "timestamp readback channel closed".to_string())?
        .map_err(|e| format!("timestamp map failed: {e}"))?;
    let mut ticks = vec![0u64; count as usize];
    {
        let data = slice.get_mapped_range();
        for (i, slot) in ticks.iter_mut().enumerate() {
            let off = i * 8;
            let mut word = [0u8; 8];
            word.copy_from_slice(&data[off..off + 8]);
            *slot = u64::from_le_bytes(word);
        }
    }
    readback.unmap();
    let ns: Vec<i64> = ticks
        .into_iter()
        .map(|t| (t as f64 * f64::from(period)) as i64)
        .collect();
    {
        let mut st = lock_state();
        if let Some(entry) = st.query_sets.get_mut(&id) {
            entry.last_ns = ns.clone();
        }
    }
    Ok(ns)
}

pub fn read(id: i32) -> Vec<i64> {
    match map_readback(id) {
        Ok(v) => v,
        Err(e) => {
            lock_state().set_last_error(e);
            Vec::new()
        }
    }
}
