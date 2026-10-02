//! Bump allocator for per-draw uniform blocks.
//!
//! Every `set_uniforms` needs its own storage, or batched draws in one submit would all read
//! whichever write landed last. Handing each one a separate `wgpu::Buffer` caps the block at the
//! buffer's size and mints a fresh bind group per draw, so instead the whole frame's uniforms go
//! into one buffer and each draw binds a window of it at a dynamic offset. Bind groups then depend
//! only on the pipeline and group, not on which draw is using them, so they survive in the cache.

use std::convert::TryFrom;

/// Size of the first allocation; the ring only ever grows, and it is reused across frames.
const INITIAL_BYTES: u64 = 64 * 1024;

#[derive(Default)]
pub struct UniformRing {
    buffer: Option<wgpu::Buffer>,
    capacity: u64,
    /// Bytes handed out so far this frame.
    cursor: u64,
    /// `min_uniform_buffer_offset_alignment` for the device — dynamic offsets must be a multiple.
    alignment: u64,
}

impl UniformRing {
    /// Starts a frame with room for `bytes` of uniform data. Returns whether the backing buffer
    /// was replaced, which invalidates every bind group built against the old one.
    ///
    /// The whole frame's requirement is reserved up front rather than grown on demand: a bind
    /// group holds the buffer it was built from, so growing partway through a frame would leave
    /// groups already built reading the old buffer at offsets only the new one was written at.
    #[must_use]
    pub fn begin_frame(&mut self, device: &wgpu::Device, bytes: u64) -> bool {
        self.cursor = 0;
        self.alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(1);
        if bytes <= self.capacity {
            return false;
        }
        let capacity = bytes.max(INITIAL_BYTES).next_power_of_two();
        self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dream-uniform-ring"),
            size: capacity,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        self.capacity = capacity;
        true
    }

    /// Bytes one block of `size` consumes, dynamic-offset alignment included.
    pub fn stride(&self, size: u32) -> u64 {
        u64::from(size.max(1)).next_multiple_of(self.alignment.max(1))
    }

    /// Alignment a frame's reservation must be computed against before `begin_frame` has run.
    pub fn stride_for(size: u32, device: &wgpu::Device) -> u64 {
        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(1);
        u64::from(size.max(1)).next_multiple_of(alignment)
    }

    /// Writes `block` and returns the dynamic offset to bind it at.
    ///
    /// `block` is padded up to `size` so a caller that packed only the leading fields still gets a
    /// legal binding window; packing *more* than the shader declares is an error rather than a
    /// silent truncation, since it means the Dream and WGSL sides disagree on the layout.
    pub fn push(
        &mut self,
        queue: &wgpu::Queue,
        block: &[u8],
        size: u32,
    ) -> Result<u32, String> {
        if block.len() > size as usize {
            return Err(format!(
                "packed {} bytes of uniforms but the shader's block is {size} bytes",
                block.len()
            ));
        }
        let stride = self.stride(size);
        let offset = self.cursor;
        let buffer = self
            .buffer
            .as_ref()
            .filter(|_| offset + stride <= self.capacity)
            .ok_or_else(|| "uniform ring was not reserved for this frame".to_string())?;
        self.cursor = offset + stride;
        let mut padded = vec![0u8; size as usize];
        padded[..block.len()].copy_from_slice(block);
        queue.write_buffer(buffer, offset, &padded);
        u32::try_from(offset).map_err(|_| "uniform ring offset exceeds u32".to_string())
    }

    /// The buffer the returned offsets index into. `None` before the first `begin_frame`.
    pub fn buffer(&self) -> Option<&wgpu::Buffer> {
        self.buffer.as_ref()
    }
}
