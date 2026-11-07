use crate::wire::{Wire, kind};
use sdl3::video::Window;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Metrics {
    pub width: f32,
    pub height: f32,
    pub pixels: (u32, u32),
    pub scale: f32,
    pub coordinates: f32,
    pub insets: [f32; 4],
}

impl Metrics {
    pub fn read(window: &Window) -> Self {
        let (width, height) = window.size();
        let scale = positive(window.display_scale());
        // SDL window units are points on macOS/Wayland and pixels on Windows/X11.
        let coordinates = scale / positive(window.pixel_density());
        let insets = window.safe_area().map_or([0.0; 4], |rect| {
            [
                rect.y() as f32,
                (width as f32 - rect.right() as f32).max(0.0),
                (height as f32 - rect.bottom() as f32).max(0.0),
                rect.x() as f32,
            ]
            .map(|value| value.max(0.0) / coordinates)
        });
        Self {
            width: width as f32 / coordinates,
            height: height as f32 / coordinates,
            pixels: window.size_in_pixels(),
            scale,
            coordinates,
            insets,
        }
    }

    pub fn changes(self, previous: Self, wire: &mut Wire) {
        if self.scale != previous.scale {
            wire.push(kind::SCALE_CHANGED, &[self.scale]);
        }
        if self.pixels != previous.pixels {
            wire.push(kind::RESIZED, &[self.pixels.0 as f32, self.pixels.1 as f32]);
        }
        if self.insets != previous.insets {
            wire.push(kind::SAFE_AREA_CHANGED, &self.insets);
        }
    }
}

fn positive(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scale_precedes_resize_and_insets_use_logical_units() {
        let before = Metrics {
            width: 100.0,
            height: 100.0,
            pixels: (100, 100),
            scale: 1.0,
            coordinates: 1.0,
            insets: [0.0; 4],
        };
        let after = Metrics {
            pixels: (200, 200),
            scale: 2.0,
            insets: [10.0, 0.0, 0.0, 0.0],
            ..before
        };
        let mut wire = Wire::default();
        after.changes(before, &mut wire);
        assert_eq!(
            wire.values(),
            &[
                20.0, 1.0, 2.0, 1.0, 2.0, 200.0, 200.0, 25.0, 4.0, 10.0, 0.0, 0.0, 0.0
            ]
        );
    }
}
