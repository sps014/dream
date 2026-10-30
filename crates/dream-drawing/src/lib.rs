//! Shared CPU/GPU drawing host. This is separate from Dream's C guest runtime.

pub mod assets;
mod commands;
mod gpu;
mod replay;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
pub mod text;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod visual_tests;
#[cfg(target_arch = "wasm32")]
mod web;

pub use commands::{Command, Picture};
use vello_cpu::{Pixmap, RenderContext, Resources};

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
    Cpu {
        width: u16,
        height: u16,
        pixels: Pixmap,
        resources: Box<Resources>,
    },
    Gpu(Box<gpu::GpuSurface>),
}

impl Surface {
    pub async fn create(width: i32, height: i32, backend: i32) -> Result<Self> {
        let (width, height) = dimensions(width, height)?;
        match backend {
            1 => Ok(Self::Cpu {
                width,
                height,
                pixels: Pixmap::new(width, height),
                resources: Box::new(Resources::new()),
            }),
            0 | 2 => match gpu::GpuSurface::new(width, height).await {
                Ok(gpu) => Ok(Self::Gpu(Box::new(gpu))),
                Err(_) if backend == 0 => Ok(Self::Cpu {
                    width,
                    height,
                    pixels: Pixmap::new(width, height),
                    resources: Box::new(Resources::new()),
                }),
                Err(error) => Err(error),
            },
            _ => Err(DrawingError::Invalid("unknown drawing backend".into())),
        }
    }

    pub fn backend(&self) -> i32 {
        if matches!(self, Self::Cpu { .. }) {
            1
        } else {
            2
        }
    }

    pub fn render(&mut self, picture: &Picture) -> Result<()> {
        match self {
            Self::Cpu {
                width,
                height,
                pixels,
                resources,
            } => {
                let settings = vello_cpu::RenderSettings::default();
                #[cfg(not(target_arch = "wasm32"))]
                let settings = vello_cpu::RenderSettings {
                    // Vello's current filter implementation requires its single-threaded
                    // dispatcher. Other pictures can use the parallel CPU pipeline.
                    num_threads: if picture
                        .commands
                        .iter()
                        .any(|command| matches!(command, Command::Backdrop { .. }))
                    {
                        0
                    } else {
                        std::thread::available_parallelism()
                            .map_or(0, |count| count.get().saturating_sub(1).min(8) as u16)
                    },
                    ..settings
                };
                let mut context = RenderContext::new_with(*width, *height, settings);
                replay::cpu(
                    &mut context,
                    resources,
                    &picture.commands,
                    *width,
                    *height,
                    &Default::default(),
                )?;
                context.flush();
                // A fresh pixmap gives every render replacement semantics, including transparent clear.
                let mut next = Pixmap::new(*width, *height);
                context.render(&mut next, resources);
                *pixels = next;
                Ok(())
            }
            Self::Gpu(gpu) => gpu.render(picture),
        }
    }

    pub async fn pixels(&self) -> Result<Vec<u8>> {
        match self {
            Self::Cpu { pixels, .. } => Ok(pixels
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
