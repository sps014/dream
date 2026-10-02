//! Recorded command stream replay: one `gpuEncoderSubmit` call runs every pass and draw the
//! frame recorded, against a single `wgpu::CommandEncoder`.

pub mod attach;
pub mod binds;
pub mod decode;

mod replay;

/// Replays `stream`, returning `0` on success or a `GpuError` code.
pub fn submit(stream: &[u8]) -> i32 {
    match replay::submit(stream) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Dream gpuEncoderSubmit: {e}");
            super::error::classify_err(&e)
        }
    }
}
