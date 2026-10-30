use crate::{DrawingError, Result, dimensions};
use std::{io::Cursor, sync::Arc};
use vello_common::pixmap::Pixmap;

#[derive(Clone)]
pub enum Asset {
    Image(Arc<RasterImage>),
    Text(Arc<crate::text::ShapedText>),
}

pub struct RasterImage {
    pub pixels: Arc<Pixmap>,
}

impl RasterImage {
    pub fn decode(bytes: &[u8]) -> Result<Arc<Self>> {
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(DrawingError::Invalid("encoded image exceeds 64 MiB".into()));
        }
        let mut reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|error| DrawingError::Invalid(error.to_string()))?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|error| DrawingError::Invalid(error.to_string()))?
            .to_rgba8();
        Self::rgba(image.width() as i32, image.height() as i32, image.as_raw())
    }

    pub fn rgba(width: i32, height: i32, bytes: &[u8]) -> Result<Arc<Self>> {
        let (width, height) = dimensions(width, height)?;
        if bytes.len() != usize::from(width) * usize::from(height) * 4 {
            return Err(DrawingError::Invalid(
                "RGBA pixels must contain width * height * 4 bytes".into(),
            ));
        }
        let mut pixels = Pixmap::new(width, height);
        for (pixel, bytes) in pixels.data_mut().iter_mut().zip(bytes.as_chunks::<4>().0) {
            let a = u16::from(bytes[3]);
            pixel.r = ((u16::from(bytes[0]) * a + 127) / 255) as u8;
            pixel.g = ((u16::from(bytes[1]) * a + 127) / 255) as u8;
            pixel.b = ((u16::from(bytes[2]) * a + 127) / 255) as u8;
            pixel.a = bytes[3];
        }
        pixels.recompute_may_have_transparency();
        Ok(Arc::new(Self {
            pixels: Arc::new(pixels),
        }))
    }
}

impl Asset {
    pub fn metrics(&self) -> Vec<f32> {
        match self {
            Self::Image(image) => vec![
                f32::from(image.pixels.width()),
                f32::from(image.pixels.height()),
            ],
            Self::Text(text) => vec![text.width, text.height],
        }
    }
}
