//! Buffers, textures, samplers and their CPU transfers. Writes are queued in submission order;
//! reads resolve their future once the GPU finished every earlier submission.

use crate::backend::Backend;
use crate::error::{self, Code, HostError, HostResult, check, feature};
use crate::ffi;
use crate::guest::OutBytes;
use crate::state::{self, BufferEntry, DeviceCore, Entry, Kind, Obj, State, TextureEntry};
use crate::waiter;
use std::sync::Arc;

/// Records with `record` into a fresh encoder and submits; returns the submission's fence value.
pub fn encode(
    core: &DeviceCore,
    record: impl FnOnce(*mut ffi::DgpuEncoder) -> HostResult<()>,
) -> HostResult<u64> {
    let mut encoder = std::ptr::null_mut();
    check(unsafe { ffi::dgpu_encoder_begin(core.raw(), &mut encoder) })?;
    if let Err(e) = record(encoder) {
        unsafe { ffi::dgpu_encoder_abort(encoder) };
        return Err(e);
    }
    let mut fence = 0;
    check(unsafe { ffi::dgpu_encoder_submit(encoder, &mut fence) })?;
    Ok(fence)
}

/// WebGPU sizes buffers, fills, writes and copies in multiples of 4 bytes. Every backend
/// allocates the padded size, so `float16` and byte buffers of any length work everywhere.
fn padded(size: u64) -> u64 {
    size.next_multiple_of(4)
}

/// The bytes a write of `data` at `offset` sends to a buffer of `size` bytes. A write that
/// runs to the end of the buffer is zero-padded into the allocation's slack. Other writes that
/// start or end inside a 4-byte word cannot be expressed on WebGPU.
fn write_bytes(
    backend: Backend,
    offset: u64,
    data: &[u8],
    size: u64,
) -> HostResult<std::borrow::Cow<'_, [u8]>> {
    let len = data.len() as u64;
    let mut bytes = std::borrow::Cow::Borrowed(data);
    if offset.is_multiple_of(4) && !len.is_multiple_of(4) && offset + len == size {
        let mut owned = data.to_vec();
        owned.resize(padded(len) as usize, 0);
        bytes = std::borrow::Cow::Owned(owned);
    }
    if backend == Backend::WebGpu && (!offset.is_multiple_of(4) || !bytes.len().is_multiple_of(4)) {
        return Err(HostError::invalid(
            "WebGPU buffer writes must start at a multiple of 4 bytes and end at one or at the end of the buffer",
        ));
    }
    Ok(bytes)
}

fn non_negative(value: i32, what: &str) -> HostResult<u64> {
    u64::try_from(value).map_err(|_| HostError::invalid(format!("{what} must not be negative")))
}

fn new_buffer(core: &DeviceCore, size: u64, usage: u32, readback: bool) -> HostResult<Obj> {
    let mut raw = std::ptr::null_mut();
    check(unsafe { ffi::dgpu_buffer_create(core.raw(), size, usage, readback as i32, &mut raw) })?;
    Ok(Obj::from_raw(raw))
}

pub fn buffer_create(device: i32, byte_size: i32, usage: i32) -> HostResult<i32> {
    let size = non_negative(byte_size, "buffer size")?;
    if usage & !0xF != 0 {
        return Err(HostError::invalid(format!("unknown buffer usage {usage:#x}")));
    }
    let mut state = state::lock();
    let core = state.device(device)?;
    let obj = new_buffer(&core, padded(size), usage as u32, false)?;
    state.insert(Entry {
        kind: Kind::Buffer(BufferEntry { obj, size }),
        device,
        last_use: 0,
        core,
    })
}

fn buffer(state: &State, handle: i32) -> HostResult<(&Entry, &BufferEntry)> {
    let entry = state.entry(handle)?;
    match &entry.kind {
        Kind::Buffer(buffer) => Ok((entry, buffer)),
        other => Err(HostError::invalid(format!(
            "handle {handle} is a {}, not a buffer",
            other.name()
        ))),
    }
}

fn texture(state: &State, handle: i32) -> HostResult<(&Entry, &TextureEntry)> {
    let entry = state.entry(handle)?;
    match &entry.kind {
        Kind::Texture(texture) => Ok((entry, texture)),
        other => Err(HostError::invalid(format!(
            "handle {handle} is a {}, not a texture",
            other.name()
        ))),
    }
}

fn range(offset: i32, size: u64, capacity: u64) -> HostResult<u64> {
    let offset = non_negative(offset, "byte offset")?;
    if offset.checked_add(size).is_none_or(|end| end > capacity) {
        return Err(HostError::invalid("byte range lies outside the buffer"));
    }
    Ok(offset)
}

pub fn buffer_write(handle: i32, byte_offset: i32, data: &[u8]) -> HostResult<()> {
    let mut state = state::lock();
    let (entry, buffer) = buffer(&state, handle)?;
    entry.core.alive()?;
    let offset = range(byte_offset, data.len() as u64, buffer.size)?;
    if data.is_empty() {
        return Ok(());
    }
    let data = write_bytes(entry.core.backend, offset, data, buffer.size)?;
    let raw = buffer.obj.raw();
    let fence = encode(&entry.core, |e| {
        check(unsafe {
            ffi::dgpu_enc_write_buffer(e, raw, offset, data.as_ptr().cast(), data.len() as u64)
        })
    })?;
    state.touch(handle, fence);
    Ok(())
}

/// A readback buffer and where its rows go: (row pitch, row bytes, rows).
struct Readback {
    staging: Obj,
    out: OutBytes,
    rows: (u64, u64, u32),
}

/// Queues a job that copies the readback rows into the guest array once `fence` is reached.
fn finish_read(
    core: Arc<DeviceCore>,
    future: usize,
    fence: u64,
    readback: Readback,
) -> HostResult<()> {
    let Readback {
        staging,
        mut out,
        rows,
    } = readback;
    core.clone().push(
        fence,
        future,
        Box::new(move |_, waited| {
            let copied = waited.and_then(|_| {
                check(unsafe {
                    ffi::dgpu_buffer_read_mapped(
                        core.raw(),
                        staging.raw(),
                        out.data_mut().as_mut_ptr().cast(),
                        rows.0,
                        rows.1,
                        rows.2,
                    )
                })
            });
            drop(staging);
            match copied {
                Ok(()) => out.finish(),
                Err(e) => {
                    error::record(&e);
                    out.fail()
                }
            }
        }),
    );
    Ok(())
}

pub fn buffer_read(future: usize, handle: i32, byte_offset: i32, byte_size: i32) {
    if let Err(e) = buffer_read_now(future, handle, byte_offset, byte_size) {
        error::record(&e);
        complete_empty(future);
    }
}

fn complete_empty(future: usize) {
    let value = OutBytes::alloc(0).map_or(0, OutBytes::finish);
    waiter::complete(future, value);
}

fn buffer_read_now(future: usize, handle: i32, byte_offset: i32, byte_size: i32) -> HostResult<()> {
    let mut state = state::lock();
    let (entry, buffer) = buffer(&state, handle)?;
    entry.core.alive()?;
    let size = non_negative(byte_size, "read size")?;
    let offset = range(byte_offset, size, buffer.size)?;
    let out = OutBytes::alloc(size as usize)
        .ok_or_else(|| HostError::new(Code::OutOfMemory, "the read result does not fit in memory"))?;
    if size == 0 {
        waiter::complete(future, out.finish());
        return Ok(());
    }
    if entry.core.backend == Backend::WebGpu && !offset.is_multiple_of(4) {
        return Err(HostError::invalid(
            "WebGPU buffer reads must start at a multiple of 4 bytes",
        ));
    }
    // An aligned copy may run into the allocation's padding, which `padded` guarantees exists.
    let copied = if offset.is_multiple_of(4) { padded(size) } else { size };
    let core = entry.core.clone();
    let source = buffer.obj.raw();
    let staging = new_buffer(&core, copied, 0, true)?;
    let target = staging.raw();
    let fence = encode(&core, |e| {
        check(unsafe { ffi::dgpu_enc_copy_buffer(e, target, 0, source, offset, copied) })
    })?;
    state.touch(handle, fence);
    let readback = Readback {
        staging,
        out,
        rows: (copied, size, 1),
    };
    finish_read(core, future, fence, readback)
}

/// `desc` is [width, height, depth_or_layers, mip_levels, format, usage, sample_count].
pub fn texture_create(device: i32, desc: &[i32]) -> HostResult<i32> {
    let &[width, height, layers, mips, format, usage, samples] = desc else {
        return Err(HostError::invalid("a texture description has 7 values"));
    };
    if width <= 0 || height <= 0 || layers <= 0 || mips <= 0 {
        return Err(HostError::invalid(
            "texture sizes, layers and mip levels must be positive",
        ));
    }
    let longest = width.max(height) as u32;
    if mips as u32 > 32 - longest.leading_zeros() {
        return Err(HostError::invalid("too many mip levels for the texture size"));
    }
    if !(0..=11).contains(&format) || usage & !3 != 0 || !(samples == 1 || samples == 4) {
        return Err(HostError::invalid(
            "unknown texture format, usage or sample count",
        ));
    }
    let mut state = state::lock();
    let core = state.device(device)?;
    let ffi_desc = ffi::TextureDesc {
        width: width as u32,
        height: height as u32,
        layers: layers as u32,
        mips: mips as u32,
        format: format as u32,
        usage: usage as u32,
        samples: samples as u32,
    };
    let mut raw = std::ptr::null_mut();
    check(unsafe { ffi::dgpu_texture_create(core.raw(), &ffi_desc, &mut raw) })?;
    state.insert(Entry {
        kind: Kind::Texture(TextureEntry {
            obj: Obj::from_raw(raw),
            layers: layers as u32,
            mips: mips as u32,
            storage: usage & 2 != 0,
        }),
        device,
        last_use: 0,
        core,
    })
}

fn subresource(texture: &TextureEntry, mip: i32, layer: i32) -> HostResult<(u32, u32)> {
    let mip = u32::try_from(mip).ok().filter(|m| *m < texture.mips);
    let layer = u32::try_from(layer).ok().filter(|l| *l < texture.layers);
    match (mip, layer) {
        (Some(mip), Some(layer)) => Ok((mip, layer)),
        _ => Err(HostError::invalid("the texture has no such mip level or layer")),
    }
}

pub fn texture_write(handle: i32, mip: i32, layer: i32, data: &[u8]) -> HostResult<()> {
    let mut state = state::lock();
    let (entry, texture) = texture(&state, handle)?;
    entry.core.alive()?;
    let (mip, layer) = subresource(texture, mip, layer)?;
    let raw = texture.obj.raw();
    let fence = encode(&entry.core, |e| {
        check(unsafe {
            ffi::dgpu_enc_write_texture(e, raw, mip, layer, data.as_ptr().cast(), data.len() as u64)
        })
    })?;
    state.touch(handle, fence);
    Ok(())
}

pub fn texture_read(future: usize, handle: i32, mip: i32, layer: i32) {
    if let Err(e) = texture_read_now(future, handle, mip, layer) {
        error::record(&e);
        complete_empty(future);
    }
}

fn texture_read_now(future: usize, handle: i32, mip: i32, layer: i32) -> HostResult<()> {
    let mut state = state::lock();
    let (entry, texture) = texture(&state, handle)?;
    entry.core.alive()?;
    let (mip, layer) = subresource(texture, mip, layer)?;
    let raw = texture.obj.raw();
    let mut layout = ffi::ReadbackLayout::default();
    check(unsafe { ffi::dgpu_texture_readback_layout(raw, mip, &mut layout) })?;
    let bytes = layout.row_bytes * layout.rows as u64;
    let out = OutBytes::alloc(bytes as usize)
        .ok_or_else(|| HostError::new(Code::OutOfMemory, "the read result does not fit in memory"))?;
    let core = entry.core.clone();
    let staging = new_buffer(&core, layout.staging_size, 0, true)?;
    let target = staging.raw();
    let fence = encode(&core, |e| {
        check(unsafe { ffi::dgpu_enc_read_texture(e, raw, mip, layer, target) })
    })?;
    state.touch(handle, fence);
    let readback = Readback {
        staging,
        out,
        rows: (layout.row_pitch, layout.row_bytes, layout.rows),
    };
    finish_read(core, future, fence, readback)
}

/// `desc` follows GpuSamplerDesc.encode.
pub fn sampler_create(device: i32, desc: &[i32]) -> HostResult<i32> {
    let valid = desc.len() == 8
        && desc[..3].iter().all(|f| (0..=1).contains(f))
        && desc[3..6].iter().all(|a| (0..=2).contains(a))
        && (0..=8).contains(&desc[6])
        && desc[7] >= 0;
    if !valid {
        return Err(HostError::invalid("malformed sampler description"));
    }
    let mut state = state::lock();
    let core = state.device(device)?;
    let mut raw = std::ptr::null_mut();
    check(unsafe { ffi::dgpu_sampler_create(core.raw(), desc.as_ptr(), &mut raw) })?;
    state.insert(Entry {
        kind: Kind::Sampler(Obj::from_raw(raw)),
        device,
        last_use: 0,
        core,
    })
}

pub fn descriptor_handle(resource: i32) -> HostResult<u64> {
    let state = state::lock();
    let entry = state.entry(resource)?;
    entry.core.alive()?;
    if !entry.core.has_feature(feature::BINDLESS) {
        return Err(HostError::unsupported(feature::BINDLESS));
    }
    let (raw, kind) = match &entry.kind {
        Kind::Buffer(b) => (b.obj.raw(), 1),
        Kind::Texture(t) => (t.obj.raw(), if t.storage { 4 } else { 2 }),
        Kind::Sampler(s) => (s.raw(), 3),
        other => {
            return Err(HostError::invalid(format!(
                "a {} has no bindless handle",
                other.name()
            )));
        }
    };
    let mut bits = 0;
    check(unsafe { ffi::dgpu_descriptor_handle(entry.core.raw(), raw, kind, &mut bits) })?;
    Ok(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocations_round_up_to_whole_words() {
        assert_eq!(padded(0), 0);
        assert_eq!(padded(514), 516);
        assert_eq!(padded(516), 516);
    }

    #[test]
    fn writes_that_end_the_buffer_are_padded() {
        let data = [1u8, 2, 3, 4, 5, 6];
        let bytes = write_bytes(Backend::WebGpu, 8, &data, 14).unwrap();
        assert_eq!(&*bytes, &[1, 2, 3, 4, 5, 6, 0, 0]);
        let whole = write_bytes(Backend::WebGpu, 0, &data[..4], 14).unwrap();
        assert!(matches!(whole, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn unaligned_writes_fail_only_on_webgpu() {
        let data = [1u8, 2];
        assert!(write_bytes(Backend::WebGpu, 2, &data, 14).is_err());
        assert!(write_bytes(Backend::WebGpu, 0, &data, 14).is_err());
        assert_eq!(&*write_bytes(Backend::Metal, 2, &data, 14).unwrap(), &data);
        assert_eq!(&*write_bytes(Backend::Vulkan, 0, &data, 14).unwrap(), &data);
    }
}
