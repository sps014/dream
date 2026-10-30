//! Shared CPU/GPU drawing host. This is separate from Dream's C guest runtime.

pub mod assets;
mod commands;
mod cpu;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod effect_tests;
pub mod effects;
mod gpu;
mod gpu_color;
mod gpu_effects;
mod prepare;
mod replay;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
pub mod text;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod visual_tests;
#[cfg(target_arch = "wasm32")]
mod web;

pub use commands::{Command, Picture};
pub use effects::RenderStats;

#[derive(Debug, thiserror::Error)]
pub enum DrawingError {
    #[error("EINVAL\n{0}")]
    Invalid(String),
    #[error("EUNAVAILABLE\n{0}")]
    Unavailable(String),
    #[error("ERENDER\n{0}")]
    Render(String),
}

pub type Result<T> = std::result::Result<T, DrawingError>;

pub enum Surface {
    Cpu(Box<cpu::CpuSurface>),
    Gpu(Box<gpu::GpuSurface>),
}

impl Surface {
    pub async fn create(width: i32, height: i32, backend: i32) -> Result<Self> {
        let (width, height) = dimensions(width, height)?;
        match backend {
            1 => Ok(Self::Cpu(Box::new(cpu::CpuSurface::new(width, height)))),
            0 | 2 => match gpu::GpuSurface::new(width, height).await {
                Ok(gpu) => Ok(Self::Gpu(Box::new(gpu))),
                Err(_) if backend == 0 => {
                    Ok(Self::Cpu(Box::new(cpu::CpuSurface::new(width, height))))
                }
                Err(error) => Err(error),
            },
            _ => Err(DrawingError::Invalid("unknown drawing backend".into())),
        }
    }

    pub fn backend(&self) -> i32 {
        if matches!(self, Self::Cpu(_)) { 1 } else { 2 }
    }

    pub fn render(&mut self, picture: &Picture) -> Result<()> {
        match self {
            Self::Cpu(cpu) => cpu.render(picture),
            Self::Gpu(gpu) => gpu.render(picture),
        }
    }

    /// Host-side instrumentation for rendering benchmarks; counts explicit pixel reads on GPU.
    pub fn stats(&self) -> RenderStats {
        match self {
            Self::Cpu(cpu) => cpu.stats(),
            Self::Gpu(gpu) => gpu.stats(),
        }
    }

    pub async fn pixels(&self) -> Result<Vec<u8>> {
        match self {
            Self::Cpu(cpu) => Ok(cpu
                .pixels
                .data()
                .iter()
                .flat_map(|p| [p.r, p.g, p.b, p.a])
                .collect()),
            Self::Gpu(gpu) => gpu.pixels().await,
        }
    }
}

fn dimensions(width: i32, height: i32) -> Result<(u16, u16)> {
    if width <= 0
        || height <= 0
        || width > 8192
        || height > 8192
        || i64::from(width) * i64::from(height) > 16_777_216
    {
        return Err(DrawingError::Invalid(
            "drawing dimensions must be 1..8192 and at most 16777216 pixels".into(),
        ));
    }
    Ok((width as u16, height as u16))
}
