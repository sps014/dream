use crate::{DrawingError, Result};
use vello_common::{
    filter_effects::{EdgeMode, Filter, FilterFunction, FilterPrimitive},
    peniko::Color,
};

pub const SCRATCH_BUDGET: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug)]
pub enum Effect {
    Blur(f32),
    Shadow {
        dx: f32,
        dy: f32,
        sigma: f32,
        color: Color,
    },
    Brightness(f32),
    Contrast(f32),
    Saturation(f32),
}

impl Effect {
    pub fn filter(&self) -> Option<Filter> {
        match self {
            Self::Blur(radius) => Some(Filter::from_function(FilterFunction::Blur {
                radius: *radius,
            })),
            Self::Shadow {
                dx,
                dy,
                sigma,
                color,
            } => Some(Filter::from_primitive(FilterPrimitive::DropShadow {
                dx: *dx,
                dy: *dy,
                std_deviation: *sigma,
                color: *color,
                edge_mode: EdgeMode::None,
            })),
            _ => None,
        }
    }

    pub fn parameters(&self) -> [f32; 4] {
        match self {
            Self::Brightness(f) => [0., *f, 0., 0.],
            Self::Contrast(f) => [1., *f, 0., 0.],
            Self::Saturation(f) => [2., *f, 0., 0.],
            _ => unreachable!("spatial effects use Vello filters"),
        }
    }

    pub fn adjust(&self, color: &mut vello_common::peniko::color::PremulRgba8) {
        let alpha = f32::from(color.a);
        if alpha == 0. {
            color.r = 0;
            color.g = 0;
            color.b = 0;
            return;
        }
        let mut rgb = [
            f32::from(color.r) / alpha,
            f32::from(color.g) / alpha,
            f32::from(color.b) / alpha,
        ];
        let luminance = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
        for channel in &mut rgb {
            *channel = match self {
                Self::Brightness(f) => *channel * f,
                Self::Contrast(f) => (*channel - 0.5) * f + 0.5,
                Self::Saturation(f) => luminance + (*channel - luminance) * f,
                _ => unreachable!("spatial effects use Vello filters"),
            };
        }
        color.r = (rgb[0].clamp(0., 1.) * alpha).round() as u8;
        color.g = (rgb[1].clamp(0., 1.) * alpha).round() as u8;
        color.b = (rgb[2].clamp(0., 1.) * alpha).round() as u8;
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct RenderStats {
    /// Cumulative full-viewport scratch target allocations, not Vello's internal caches.
    pub scratch_allocations: u64,
    pub scratch_bytes: usize,
    pub pixel_reads: u64,
}

pub fn reserve(stats: &mut RenderStats, width: u16, height: u16) -> Result<()> {
    let bytes = usize::from(width) * usize::from(height) * 4;
    if stats.scratch_bytes + bytes > SCRATCH_BUDGET {
        return Err(DrawingError::Render(
            "filter scratch targets exceed 256 MiB".into(),
        ));
    }
    stats.scratch_bytes += bytes;
    stats.scratch_allocations += 1;
    Ok(())
}
