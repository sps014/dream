//! The `float[]` record stream `windowPoll` returns. `system/window/input_wire.dream` decodes it
//! and `runtime/src/hosts/window.js` produces the same records in browsers.
//!
//! Each record is `[kind, length, payload...]`. Text is UTF-16 code units, which `f32` holds
//! exactly, so one array carries every event without a second allocation per string.

pub mod kind {
    pub const RESIZED: u8 = 1;
    pub const MOVED: u8 = 2;
    pub const FOCUSED: u8 = 3;
    pub const UNFOCUSED: u8 = 4;
    pub const CLOSE_REQUESTED: u8 = 5;
    pub const KEY_DOWN: u8 = 6;
    pub const KEY_UP: u8 = 7;
    pub const TEXT: u8 = 8;
    pub const MOUSE_MOVE: u8 = 9;
    pub const MOUSE_DOWN: u8 = 10;
    pub const MOUSE_UP: u8 = 11;
    pub const SCROLL: u8 = 12;
    pub const GAMEPAD_CONNECTED: u8 = 13;
    pub const GAMEPAD_DISCONNECTED: u8 = 14;
    pub const FILE_DROPPED: u8 = 15;
    pub const GAMEPAD_SAMPLE: u8 = 16;
    pub const MINIMIZED: u8 = 17;
    pub const RESTORED: u8 = 18;
    pub const OCCLUDED: u8 = 19;
    pub const SCALE_CHANGED: u8 = 20;
    pub const TOUCH_DOWN: u8 = 21;
    pub const TOUCH_MOVE: u8 = 22;
    pub const TOUCH_UP: u8 = 23;
    pub const TOUCH_CANCEL: u8 = 24;
    pub const SAFE_AREA_CHANGED: u8 = 25;
}

#[derive(Default, Debug)]
pub struct Wire {
    values: Vec<f32>,
}

impl Wire {
    pub fn push(&mut self, kind: u8, payload: &[f32]) {
        self.values.push(f32::from(kind));
        self.values.push(payload.len() as f32);
        self.values.extend_from_slice(payload);
    }

    pub fn push_text(&mut self, kind: u8, prefix: &[f32], text: &str) {
        let units: Vec<f32> = text.encode_utf16().map(f32::from).collect();
        self.values.push(f32::from(kind));
        self.values.push((prefix.len() + units.len()) as f32);
        self.values.extend_from_slice(prefix);
        self.values.extend(units);
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }

    pub fn into_values(self) -> Vec<f32> {
        self.values
    }
}
