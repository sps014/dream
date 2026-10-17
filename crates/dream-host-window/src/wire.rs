//! The `float[]` record stream `windowPoll` returns. `system/window/input_wire.dream` decodes it
//! and `runtime/src/hosts/window.js` produces the same records in browsers.
//!
//! Each record is `[kind, length, payload...]`. Text is UTF-16 code units, which `f32` holds
//! exactly, so one array carries every event without a second allocation per string.

use glfw::{Action, Key, Scancode, WindowEvent};

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
}

/// `MouseButton` has five choices; GLFW's extra buttons have no Dream name.
const MOUSE_BUTTONS: i32 = 5;
/// `KeyModifiers` covers Shift through Num Lock, GLFW's low six modifier bits.
const MODIFIER_MASK: i32 = 63;

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

/// Appends the record for one GLFW event. `coord_scale` converts GLFW screen coordinates into
/// Dream's logical pixels; `key_name` asks the active layout what a key prints.
pub fn encode_event(
    event: &WindowEvent,
    coord_scale: f64,
    key_name: &dyn Fn(Key, Scancode) -> Option<String>,
    wire: &mut Wire,
) {
    match event {
        WindowEvent::FramebufferSize(w, h) => wire.push(kind::RESIZED, &[*w as f32, *h as f32]),
        WindowEvent::Pos(x, y) => wire.push(kind::MOVED, &[*x as f32, *y as f32]),
        WindowEvent::Focus(true) => wire.push(kind::FOCUSED, &[]),
        WindowEvent::Focus(false) => wire.push(kind::UNFOCUSED, &[]),
        WindowEvent::Close => wire.push(kind::CLOSE_REQUESTED, &[]),
        WindowEvent::Key(key, scancode, action, modifiers) => {
            let (record, repeat) = match action {
                Action::Press => (kind::KEY_DOWN, 0.0),
                Action::Repeat => (kind::KEY_DOWN, 1.0),
                Action::Release => (kind::KEY_UP, 0.0),
            };
            let code = f32::from(crate::keys::key_code(*key));
            let mods = (modifiers.bits() & MODIFIER_MASK) as f32;
            let name = key_name(*key, *scancode).unwrap_or_default();
            wire.push_text(record, &[code, mods, repeat], &name);
        }
        WindowEvent::Char(c) => wire.push_text(kind::TEXT, &[], c.encode_utf8(&mut [0; 4])),
        WindowEvent::CursorPos(x, y) => wire.push(
            kind::MOUSE_MOVE,
            &[(x / coord_scale) as f32, (y / coord_scale) as f32],
        ),
        WindowEvent::MouseButton(button, action, _) => {
            let index = *button as i32;
            if index < MOUSE_BUTTONS {
                let record = if *action == Action::Release {
                    kind::MOUSE_UP
                } else {
                    kind::MOUSE_DOWN
                };
                wire.push(record, &[index as f32]);
            }
        }
        WindowEvent::Scroll(x, y) => wire.push(kind::SCROLL, &[*x as f32, *y as f32]),
        WindowEvent::FileDrop(paths) => {
            for path in paths {
                wire.push_text(kind::FILE_DROPPED, &[], &path.to_string_lossy());
            }
        }
        _ => {}
    }
}

/// Appends one gamepad sample: a button bitmask in `GamepadButton` order and six axes. GLFW
/// reports triggers from -1 (released) to 1; Dream triggers run from 0 to 1.
pub fn encode_gamepad_sample(slot: i32, buttons: u32, raw_axes: [f32; 6], wire: &mut Wire) {
    let mut payload = [0.0; 8];
    payload[0] = slot as f32;
    payload[1] = buttons as f32;
    for (index, value) in raw_axes.iter().enumerate() {
        payload[2 + index] = if index >= 4 {
            ((value + 1.0) / 2.0).clamp(0.0, 1.0)
        } else {
            value.clamp(-1.0, 1.0)
        };
    }
    wire.push(kind::GAMEPAD_SAMPLE, &payload);
}

#[cfg(test)]
mod tests {
    use super::*;
    use glfw::{Modifiers, MouseButton};
    use std::path::PathBuf;

    fn encode_all(events: &[WindowEvent], scale: f64) -> Vec<f32> {
        let name = |key: Key, _| (key == Key::Q).then(|| "a".to_string());
        let mut wire = Wire::default();
        for event in events {
            encode_event(event, scale, &name, &mut wire);
        }
        wire.into_values()
    }

    #[test]
    fn synthetic_keyboard_events_keep_physical_codes_and_layout_names() {
        let values = encode_all(
            &[
                WindowEvent::Key(Key::Q, 12, Action::Press, Modifiers::Shift),
                WindowEvent::Key(Key::Q, 12, Action::Repeat, Modifiers::Shift),
                WindowEvent::Key(Key::Escape, 53, Action::Release, Modifiers::empty()),
                WindowEvent::Char('é'),
                WindowEvent::Char('😀'),
            ],
            1.0,
        );
        assert_eq!(
            values,
            vec![
                6.0, 4.0, 17.0, 1.0, 0.0, 97.0, // KeyDown Q, Shift, prints "a"
                6.0, 4.0, 17.0, 1.0, 1.0, 97.0, // repeat
                7.0, 3.0, 61.0, 0.0, 0.0, // KeyUp Escape, no name
                8.0, 1.0, 233.0, // "é"
                8.0, 2.0, 55357.0, 56832.0, // surrogate pair
            ]
        );
    }

    #[test]
    fn synthetic_pointer_events_use_logical_coordinates() {
        let values = encode_all(
            &[
                WindowEvent::CursorPos(300.0, 150.0),
                WindowEvent::MouseButton(MouseButton::Button1, Action::Press, Modifiers::empty()),
                WindowEvent::MouseButton(MouseButton::Button2, Action::Release, Modifiers::empty()),
                WindowEvent::MouseButton(MouseButton::Button8, Action::Press, Modifiers::empty()),
                WindowEvent::Scroll(0.0, -2.0),
            ],
            1.5,
        );
        assert_eq!(
            values,
            vec![
                9.0, 2.0, 200.0, 100.0, 10.0, 1.0, 0.0, 11.0, 1.0, 1.0, 12.0, 2.0, 0.0, -2.0
            ]
        );
    }

    #[test]
    fn synthetic_window_events_map_one_to_one() {
        let values = encode_all(
            &[
                WindowEvent::FramebufferSize(1600, 1200),
                WindowEvent::Size(800, 600),
                WindowEvent::Pos(10, 20),
                WindowEvent::Focus(false),
                WindowEvent::Focus(true),
                WindowEvent::Close,
                WindowEvent::FileDrop(vec![PathBuf::from("/a"), PathBuf::from("b")]),
            ],
            1.0,
        );
        assert_eq!(
            values,
            vec![
                1.0, 2.0, 1600.0, 1200.0, 2.0, 2.0, 10.0, 20.0, 4.0, 0.0, 3.0, 0.0, 5.0, 0.0, 15.0,
                2.0, 47.0, 97.0, 15.0, 1.0, 98.0,
            ]
        );
    }

    #[test]
    fn gamepad_triggers_are_normalized() {
        let mut wire = Wire::default();
        encode_gamepad_sample(2, 0b101, [0.5, -2.0, 0.0, 1.0, -1.0, 1.0], &mut wire);
        assert_eq!(
            wire.values(),
            &[16.0, 8.0, 2.0, 5.0, 0.5, -1.0, 0.0, 1.0, 0.0, 1.0]
        );
    }
}
