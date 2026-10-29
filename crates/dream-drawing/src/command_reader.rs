use super::Brush;
use crate::{DrawingError, Result};
use vello_common::{
    kurbo::{BezPath, Point},
    peniko::{Color, Gradient},
};

pub(super) struct Reader<'a> {
    pub(super) values: &'a [f32],
    pub(super) offset: usize,
}

pub(super) fn invalid(message: &str) -> DrawingError {
    DrawingError::Invalid(message.into())
}

impl Reader<'_> {
    pub(super) fn string(&mut self, limit: usize) -> Result<String> {
        let length = self.count(limit)?;
        let units = (0..length)
            .map(|_| self.count(65535).map(|unit| unit as u16))
            .collect::<Result<Vec<_>>>()?;
        Ok(String::from_utf16_lossy(&units))
    }

    pub(super) fn rect(&mut self) -> Result<vello_common::kurbo::Rect> {
        let origin = self.point()?;
        let width = self.value()?;
        let height = self.value()?;
        if width <= 0.0 || height <= 0.0 {
            return Err(invalid("image rectangle dimensions must be positive"));
        }
        Ok(vello_common::kurbo::Rect::new(
            origin.x,
            origin.y,
            origin.x + width as f64,
            origin.y + height as f64,
        ))
    }
    pub(super) fn value(&mut self) -> Result<f32> {
        let value = *self
            .values
            .get(self.offset)
            .ok_or_else(|| invalid("truncated drawing command"))?;
        self.offset += 1;
        if !value.is_finite() || value.abs() > 1_000_000.0 {
            return Err(invalid(
                "drawing values must be finite and within +/-1000000",
            ));
        }
        Ok(value)
    }
    pub(super) fn count(&mut self, max: usize) -> Result<usize> {
        let value = self.value()?;
        if value < 0.0 || value.fract() != 0.0 || value > max as f32 {
            return Err(invalid("invalid drawing count or opcode"));
        }
        Ok(value as usize)
    }
    pub(super) fn point(&mut self) -> Result<Point> {
        Ok(Point::new(self.value()? as f64, self.value()? as f64))
    }
    pub(super) fn color(&mut self) -> Result<Color> {
        let mut values = [0.0; 4];
        for value in &mut values {
            *value = self.value()?;
            if !(0.0..=1.0).contains(value) {
                return Err(invalid("color components must be in 0..1"));
            }
        }
        Ok(Color::new(values))
    }
    pub(super) fn path(&mut self) -> Result<BezPath> {
        let count = self.count(65536)?;
        let mut path = BezPath::new();
        let mut moved = false;
        for _ in 0..count {
            let opcode = self.count(4)?;
            if opcode != 0 && !moved {
                return Err(invalid("a path must begin with move_to"));
            }
            match opcode {
                0 => {
                    path.move_to(self.point()?);
                    moved = true;
                }
                1 => path.line_to(self.point()?),
                2 => {
                    let a = self.point()?;
                    path.quad_to(a, self.point()?);
                }
                3 => {
                    let a = self.point()?;
                    let b = self.point()?;
                    path.curve_to(a, b, self.point()?);
                }
                4 => path.close_path(),
                _ => unreachable!(),
            }
        }
        Ok(path)
    }
    pub(super) fn brush(&mut self) -> Result<Brush> {
        let kind = self.count(3)?;
        if kind == 0 {
            return Ok(Brush::Solid(self.color()?));
        }
        let a = self.point()?;
        let mut gradient = match kind {
            1 => Gradient::new_linear(a, self.point()?),
            2 => {
                let radius = self.value()?;
                if radius <= 0.0 {
                    return Err(invalid("gradient radius must be positive"));
                }
                Gradient::new_radial(a, radius)
            }
            3 => Gradient::new_sweep(a, self.value()?, self.value()?),
            _ => unreachable!(),
        };
        let count = self.count(256)?;
        if count < 2 {
            return Err(invalid("a gradient needs at least two stops"));
        }
        let mut stops = Vec::with_capacity(count);
        let mut previous = 0.0;
        for _ in 0..count {
            let offset = self.value()?;
            if !(previous..=1.0).contains(&offset) {
                return Err(invalid("gradient stops must be sorted within 0..1"));
            }
            previous = offset;
            stops.push((offset, self.color()?));
        }
        gradient = gradient.with_stops(stops.as_slice());
        Ok(Brush::Gradient(gradient))
    }
}
