use super::Brush;
use crate::{DrawingError, Result};
use vello_common::{
    kurbo::{BezPath, Cap, Join, Point, Rect, RoundedRect, RoundedRectRadii, Shape, Stroke},
    peniko::{BlendMode, Color, Compose, Gradient, Mix},
};

pub(super) struct Reader<'a> {
    pub(super) values: &'a [f32],
    pub(super) offset: usize,
}

pub(super) fn invalid(message: &str) -> DrawingError {
    DrawingError::Invalid(message.into())
}

impl Reader<'_> {
    pub(super) fn effects(&mut self) -> Result<Vec<crate::effects::Effect>> {
        use crate::effects::Effect;
        let count = self.count(16)?;
        (0..count).map(|_| {
            let kind=self.count(4)?;
            if kind==1 {
                let dx=self.value()?;
                let dy=self.value()?;
                let sigma=self.value()?;
                if dx.abs()>8192.||dy.abs()>8192.||!(0.0..=128.0).contains(&sigma) {
                    return Err(invalid("shadow offsets must be within +/-8192 and sigma in 0..128 physical pixels"));
                }
                return Ok(Effect::Shadow {dx,dy,sigma,color:self.color()?});
            }
            let value=self.value()?;
            let max=if kind==0 {128.} else {8.};
            if !(0.0..=max).contains(&value) {return Err(invalid("blur sigma must be in 0..128 and color factors in 0..8"));}
            Ok(match kind {0=>Effect::Blur(value),2=>Effect::Brightness(value),3=>Effect::Contrast(value),_=>Effect::Saturation(value)})
        }).collect()
    }

    pub(super) fn blend(&mut self) -> Result<BlendMode> {
        let mix = match self.count(7)? {
            0 => Mix::Normal,
            1 => Mix::Multiply,
            2 => Mix::Screen,
            3 => Mix::Overlay,
            4 => Mix::Darken,
            5 => Mix::Lighten,
            6 => Mix::Difference,
            _ => Mix::Exclusion,
        };
        Ok(BlendMode::new(mix, Compose::SrcOver))
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
    pub(super) fn stroke(&mut self) -> Result<Stroke> {
        let width = self.value()?;
        if width <= 0.0 {
            return Err(invalid("stroke width must be positive"));
        }
        let cap = match self.count(2)? {
            0 => Cap::Butt,
            1 => Cap::Round,
            _ => Cap::Square,
        };
        let join = match self.count(2)? {
            0 => Join::Miter,
            1 => Join::Round,
            _ => Join::Bevel,
        };
        let miter = self.value()?;
        if miter < 1.0 {
            return Err(invalid("miter limit must be at least one"));
        }
        let offset = self.value()? as f64;
        let count = self.count(256)?;
        let mut dashes = Vec::with_capacity(count);
        for _ in 0..count {
            let dash = self.value()?;
            if dash <= 0.0 {
                return Err(invalid("dash lengths must be positive"));
            }
            dashes.push(dash as f64);
        }
        Ok(Stroke::new(width as f64)
            .with_caps(cap)
            .with_join(join)
            .with_miter_limit(miter as f64)
            .with_dashes(offset, dashes))
    }

    pub(super) fn path(&mut self) -> Result<BezPath> {
        let count = self.count(65536)?;
        let mut path = BezPath::new();
        let mut moved = false;
        for _ in 0..count {
            let opcode = self.count(5)?;
            if !matches!(opcode, 0 | 5) && !moved {
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
                5 => {
                    let origin = self.point()?;
                    let size = self.point()?;
                    let mut radii = [0.0; 4];
                    for radius in &mut radii {
                        *radius = self.value()? as f64;
                        if *radius < 0.0 {
                            return Err(invalid("corner radii must not be negative"));
                        }
                    }
                    let rect = Rect::from_origin_size(origin, (size.x, size.y));
                    path.extend(
                        RoundedRect::from_rect(
                            rect,
                            RoundedRectRadii::new(radii[0], radii[1], radii[2], radii[3]),
                        )
                        .path_elements(0.1),
                    );
                    moved = true;
                }
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
            3 => {
                let start = self.value()?;
                let end = self.value()?;
                if end <= start {
                    return Err(invalid("sweep end angle must exceed start angle"));
                }
                Gradient::new_sweep(a, start, end)
            }
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
