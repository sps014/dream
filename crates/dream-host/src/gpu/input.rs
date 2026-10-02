//! Packed GPU surface input (shared wire format for native + JS hosts).
//!
//! Pointer latch (`gpuSurfacePointer`): 32 bytes LE
//!   f32 x, y, dx, dy; i32 buttons, down, inside, pointer_id
//!   (dx/dy cleared after each read)
//!
//! Multi-pointer (`gpuSurfacePointers`): u32 count LE, then `count` × 32-byte records
//!   (same layout as the primary latch; per-record dx/dy cleared on read)
//!
//! Mods (`gpuSurfaceMods`): 4 bytes — shift, ctrl, alt, meta as 0/1
//!
//! Events (`gpuSurfacePollEvents`): u32 count LE, then tagged payloads:
//!   0 PointerDown / 1 PointerUp: f32 x,y; i32 button, pointer_id
//!   2 PointerMove / 3 PointerEnter / 4 PointerLeave: f32 x,y; i32 pointer_id
//!   5 PointerCancel: i32 pointer_id
//!   6 Wheel: f32 dx,dy,x,y
//!   7 KeyDown: string code, string key, u8 repeat
//!   8 KeyUp: string code, string key
//!   9 TextInput: string text
//!   10 Resize: i32 w,h
//!   11 ScaleFactor: f32 scale
//!   12 Focus / 13 Blur / 14 Close: (no payload)
//!   15 GamepadConnected / 16 GamepadDisconnected: i32 pad
//!   17 GamepadButtonDown / 18 GamepadButtonUp: i32 pad, u8 button
//!   19 GamepadAxis: i32 pad, u8 axis, f32 value
//! Strings: u32 utf8_len LE + utf8 bytes.
//!
//! Gamepad button ids (keep in sync with `GamepadButton` in stdlib):
//!   0 Unknown, 1 South, 2 East, 3 West, 4 North,
//!   5–8 DPad Up/Down/Left/Right, 9–10 Shoulders, 11–12 Triggers,
//!   13–14 Stick clicks, 15 Start, 16 Select
//! Axes: 0–1 LeftStick X/Y, 2–3 RightStick X/Y, 4–5 Left/Right Trigger

use indexmap::IndexSet;
use std::collections::{BTreeMap, VecDeque};

pub const TAG_POINTER_DOWN: u8 = 0;
pub const TAG_POINTER_UP: u8 = 1;
pub const TAG_POINTER_MOVE: u8 = 2;
pub const TAG_POINTER_ENTER: u8 = 3;
pub const TAG_POINTER_LEAVE: u8 = 4;
pub const TAG_POINTER_CANCEL: u8 = 5;
pub const TAG_WHEEL: u8 = 6;
pub const TAG_KEY_DOWN: u8 = 7;
pub const TAG_KEY_UP: u8 = 8;
pub const TAG_TEXT_INPUT: u8 = 9;
pub const TAG_RESIZE: u8 = 10;
pub const TAG_SCALE_FACTOR: u8 = 11;
pub const TAG_FOCUS: u8 = 12;
pub const TAG_BLUR: u8 = 13;
pub const TAG_CLOSE: u8 = 14;
pub const TAG_GAMEPAD_CONNECTED: u8 = 15;
pub const TAG_GAMEPAD_DISCONNECTED: u8 = 16;
pub const TAG_GAMEPAD_BUTTON_DOWN: u8 = 17;
pub const TAG_GAMEPAD_BUTTON_UP: u8 = 18;
pub const TAG_GAMEPAD_AXIS: u8 = 19;

pub const BTN_UNKNOWN: u8 = 0;
pub const BTN_SOUTH: u8 = 1;
pub const BTN_EAST: u8 = 2;
pub const BTN_WEST: u8 = 3;
pub const BTN_NORTH: u8 = 4;
pub const BTN_DPAD_UP: u8 = 5;
pub const BTN_DPAD_DOWN: u8 = 6;
pub const BTN_DPAD_LEFT: u8 = 7;
pub const BTN_DPAD_RIGHT: u8 = 8;
pub const BTN_LEFT_SHOULDER: u8 = 9;
pub const BTN_RIGHT_SHOULDER: u8 = 10;
pub const BTN_LEFT_TRIGGER: u8 = 11;
pub const BTN_RIGHT_TRIGGER: u8 = 12;
pub const BTN_LEFT_STICK: u8 = 13;
pub const BTN_RIGHT_STICK: u8 = 14;
pub const BTN_START: u8 = 15;
pub const BTN_SELECT: u8 = 16;

pub const AXIS_LEFT_STICK_X: usize = 0;
pub const AXIS_LEFT_STICK_Y: usize = 1;
pub const AXIS_RIGHT_STICK_X: usize = 2;
pub const AXIS_RIGHT_STICK_Y: usize = 3;
pub const AXIS_LEFT_TRIGGER: usize = 4;
pub const AXIS_RIGHT_TRIGGER: usize = 5;
pub const AXIS_COUNT: usize = 6;

/// Stick deadzone applied before exposing axis values to Dream.
pub const STICK_DEADZONE: f32 = 0.15;

const MAX_EVENTS: usize = 256;

#[derive(Clone)]
pub enum InputEvent {
    PointerDown {
        x: f32,
        y: f32,
        button: i32,
        pointer_id: i32,
    },
    PointerUp {
        x: f32,
        y: f32,
        button: i32,
        pointer_id: i32,
    },
    PointerMove {
        x: f32,
        y: f32,
        pointer_id: i32,
    },
    PointerEnter {
        x: f32,
        y: f32,
        pointer_id: i32,
    },
    PointerLeave {
        x: f32,
        y: f32,
        pointer_id: i32,
    },
    PointerCancel {
        pointer_id: i32,
    },
    Wheel {
        dx: f32,
        dy: f32,
        x: f32,
        y: f32,
    },
    KeyDown {
        code: String,
        key: String,
        repeat: bool,
    },
    KeyUp {
        code: String,
        key: String,
    },
    TextInput {
        text: String,
    },
    Resize {
        width: i32,
        height: i32,
    },
    ScaleFactor {
        scale: f32,
    },
    Focus,
    Blur,
    Close,
    GamepadConnected {
        pad: i32,
    },
    GamepadDisconnected {
        pad: i32,
    },
    GamepadButtonDown {
        pad: i32,
        button: u8,
    },
    GamepadButtonUp {
        pad: i32,
        button: u8,
    },
    GamepadAxis {
        pad: i32,
        axis: u8,
        value: f32,
    },
}

#[derive(Clone, Default)]
pub struct PadState {
    pub buttons_down: IndexSet<u8>,
    pub axes: [f32; AXIS_COUNT],
}

#[derive(Clone, Default)]
pub struct TrackedPointer {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub buttons: i32,
    pub inside: bool,
}

#[derive(Clone)]
pub struct InputState {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub buttons: i32,
    pub inside: bool,
    pub pointer_id: i32,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
    pub focused: bool,
    pub close_requested: bool,
    pub keys_down: IndexSet<String>,
    /// Connected pads keyed by stable host index (sorted via BTreeMap).
    pub pads: BTreeMap<i32, PadState>,
    /// Active pointers keyed by host pointer id (mouse is 0).
    pub pointers: BTreeMap<i32, TrackedPointer>,
    queue: VecDeque<InputEvent>,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            dx: 0.0,
            dy: 0.0,
            buttons: 0,
            inside: false,
            pointer_id: -1,
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            focused: true,
            close_requested: false,
            keys_down: IndexSet::new(),
            pads: BTreeMap::new(),
            pointers: BTreeMap::new(),
            queue: VecDeque::new(),
        }
    }
}

impl InputState {
    fn push(&mut self, ev: InputEvent) {
        if self.queue.len() >= MAX_EVENTS {
            self.queue.pop_front();
        }
        self.queue.push_back(ev);
    }

    pub fn set_pointer_pos(&mut self, x: f32, y: f32) {
        self.set_pointer_pos_ex(x, y, true);
    }

    pub fn set_pointer_pos_ex(&mut self, x: f32, y: f32, accumulate_delta: bool) {
        if accumulate_delta {
            self.dx += x - self.x;
            self.dy += y - self.y;
        }
        self.x = x;
        self.y = y;
    }

    fn track_pointer(&mut self, pointer_id: i32, x: f32, y: f32, accumulate_delta: bool) {
        let entry = self.pointers.entry(pointer_id).or_default();
        if accumulate_delta {
            entry.dx += x - entry.x;
            entry.dy += y - entry.y;
        }
        entry.x = x;
        entry.y = y;
    }

    fn pointer_entry(&mut self, pointer_id: i32) -> &mut TrackedPointer {
        self.pointers.entry(pointer_id).or_default()
    }

    pub fn add_relative_delta(&mut self, dx: f32, dy: f32) {
        self.dx += dx;
        self.dy += dy;
        let id = if self.pointer_id >= 0 {
            self.pointer_id
        } else {
            0
        };
        let entry = self.pointer_entry(id);
        entry.dx += dx;
        entry.dy += dy;
    }

    pub fn pointer_down(&mut self, x: f32, y: f32, button: i32, pointer_id: i32) {
        self.set_pointer_pos(x, y);
        self.buttons |= 1 << button.clamp(0, 31);
        self.pointer_id = pointer_id;
        self.track_pointer(pointer_id, x, y, true);
        let bit = 1 << button.clamp(0, 31);
        let p = self.pointer_entry(pointer_id);
        p.buttons |= bit;
        p.inside = true;
        self.push(InputEvent::PointerDown {
            x,
            y,
            button,
            pointer_id,
        });
    }

    pub fn pointer_up(&mut self, x: f32, y: f32, button: i32, pointer_id: i32) {
        self.set_pointer_pos(x, y);
        self.buttons &= !(1 << button.clamp(0, 31));
        self.track_pointer(pointer_id, x, y, true);
        let bit = 1 << button.clamp(0, 31);
        let drop_finger = if let Some(p) = self.pointers.get_mut(&pointer_id) {
            p.buttons &= !bit;
            p.buttons == 0 && pointer_id != 0
        } else {
            false
        };
        if drop_finger {
            self.pointers.remove(&pointer_id);
        }
        self.push(InputEvent::PointerUp {
            x,
            y,
            button,
            pointer_id,
        });
    }

    pub fn pointer_move(&mut self, x: f32, y: f32, pointer_id: i32) {
        self.pointer_move_ex(x, y, pointer_id, true);
    }

    pub fn pointer_move_abs(&mut self, x: f32, y: f32, pointer_id: i32) {
        self.pointer_move_ex(x, y, pointer_id, false);
    }

    fn pointer_move_ex(&mut self, x: f32, y: f32, pointer_id: i32, accumulate_delta: bool) {
        self.set_pointer_pos_ex(x, y, accumulate_delta);
        self.pointer_id = pointer_id;
        self.track_pointer(pointer_id, x, y, accumulate_delta);
        self.push(InputEvent::PointerMove { x, y, pointer_id });
    }

    pub fn pointer_enter(&mut self, x: f32, y: f32, pointer_id: i32) {
        self.inside = true;
        self.set_pointer_pos(x, y);
        self.pointer_id = pointer_id;
        self.track_pointer(pointer_id, x, y, true);
        self.pointer_entry(pointer_id).inside = true;
        self.push(InputEvent::PointerEnter { x, y, pointer_id });
    }

    pub fn pointer_leave(&mut self, x: f32, y: f32, pointer_id: i32) {
        self.inside = false;
        self.set_pointer_pos(x, y);
        self.track_pointer(pointer_id, x, y, true);
        self.pointers.remove(&pointer_id);
        self.push(InputEvent::PointerLeave { x, y, pointer_id });
    }

    pub fn pointer_cancel(&mut self, pointer_id: i32) {
        self.buttons = 0;
        self.pointers.remove(&pointer_id);
        self.push(InputEvent::PointerCancel { pointer_id });
    }

    pub fn wheel(&mut self, dx: f32, dy: f32, x: f32, y: f32) {
        self.push(InputEvent::Wheel { dx, dy, x, y });
    }

    pub fn key_down(&mut self, code: String, key: String, repeat: bool) {
        if !repeat {
            self.keys_down.insert(code.clone());
        }
        self.push(InputEvent::KeyDown { code, key, repeat });
    }

    pub fn key_up(&mut self, code: String, key: String) {
        self.keys_down.shift_remove(&code);
        self.push(InputEvent::KeyUp { code, key });
    }

    pub fn text_input(&mut self, text: String) {
        if !text.is_empty() {
            self.push(InputEvent::TextInput { text });
        }
    }

    pub fn resize(&mut self, width: i32, height: i32) {
        self.push(InputEvent::Resize { width, height });
    }

    pub fn scale_factor(&mut self, scale: f32) {
        self.push(InputEvent::ScaleFactor { scale });
    }

    pub fn focus(&mut self) {
        self.focused = true;
        self.push(InputEvent::Focus);
    }

    pub fn blur(&mut self) {
        self.focused = false;
        self.push(InputEvent::Blur);
    }

    pub fn close(&mut self) {
        self.close_requested = true;
        self.push(InputEvent::Close);
    }

    pub fn gamepad_connected(&mut self, pad: i32) {
        self.pads.entry(pad).or_default();
        self.push(InputEvent::GamepadConnected { pad });
    }

    pub fn gamepad_disconnected(&mut self, pad: i32) {
        self.pads.remove(&pad);
        self.push(InputEvent::GamepadDisconnected { pad });
    }

    pub fn gamepad_button_down(&mut self, pad: i32, button: u8) {
        if button == BTN_UNKNOWN {
            return;
        }
        let inserted = self
            .pads
            .entry(pad)
            .or_default()
            .buttons_down
            .insert(button);
        if inserted {
            self.push(InputEvent::GamepadButtonDown { pad, button });
        }
    }

    pub fn gamepad_button_up(&mut self, pad: i32, button: u8) {
        if button == BTN_UNKNOWN {
            return;
        }
        if let Some(entry) = self.pads.get_mut(&pad) {
            if entry.buttons_down.shift_remove(&button) {
                self.push(InputEvent::GamepadButtonUp { pad, button });
            }
        }
    }

    pub fn gamepad_set_axis(&mut self, pad: i32, axis: usize, value: f32) {
        if axis >= AXIS_COUNT {
            return;
        }
        let entry = self.pads.entry(pad).or_default();
        let prev = entry.axes[axis];
        entry.axes[axis] = value;
        if (value - prev).abs() > 1e-4 {
            self.push(InputEvent::GamepadAxis {
                pad,
                axis: axis as u8,
                value,
            });
        }
    }

    pub fn pack_pointer_and_clear_delta(&mut self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32);
        out.extend_from_slice(&self.x.to_le_bytes());
        out.extend_from_slice(&self.y.to_le_bytes());
        out.extend_from_slice(&self.dx.to_le_bytes());
        out.extend_from_slice(&self.dy.to_le_bytes());
        out.extend_from_slice(&self.buttons.to_le_bytes());
        out.extend_from_slice(&(i32::from(self.buttons != 0)).to_le_bytes());
        out.extend_from_slice(&(i32::from(self.inside)).to_le_bytes());
        out.extend_from_slice(&self.pointer_id.to_le_bytes());
        self.dx = 0.0;
        self.dy = 0.0;
        out
    }

    pub fn pack_pointers_and_clear_deltas(&mut self) -> Vec<u8> {
        let ids: Vec<i32> = self.pointers.keys().copied().collect();
        let count = ids.len() as u32;
        let mut out = Vec::with_capacity(4 + ids.len() * 32);
        out.extend_from_slice(&count.to_le_bytes());
        for id in ids {
            let p = self.pointers.get_mut(&id).expect("pointer id from keys");
            out.extend_from_slice(&p.x.to_le_bytes());
            out.extend_from_slice(&p.y.to_le_bytes());
            out.extend_from_slice(&p.dx.to_le_bytes());
            out.extend_from_slice(&p.dy.to_le_bytes());
            out.extend_from_slice(&p.buttons.to_le_bytes());
            out.extend_from_slice(&(i32::from(p.buttons != 0)).to_le_bytes());
            out.extend_from_slice(&(i32::from(p.inside)).to_le_bytes());
            out.extend_from_slice(&id.to_le_bytes());
            p.dx = 0.0;
            p.dy = 0.0;
        }
        out
    }

    pub fn pack_mods(&self) -> Vec<u8> {
        vec![
            u8::from(self.shift),
            u8::from(self.ctrl),
            u8::from(self.alt),
            u8::from(self.meta),
        ]
    }

    pub fn key_is_down(&self, code: &str) -> bool {
        self.keys_down.contains(code)
    }

    pub fn connected_pads(&self) -> Vec<i32> {
        self.pads.keys().copied().collect()
    }

    pub fn gamepad_is_connected(&self, pad: i32) -> bool {
        self.pads.contains_key(&pad)
    }

    pub fn gamepad_button_is_down(&self, pad: i32, button: u8) -> bool {
        self.pads
            .get(&pad)
            .is_some_and(|p| p.buttons_down.contains(&button))
    }

    pub fn gamepad_axis_value(&self, pad: i32, axis: i32) -> f32 {
        if !(0..AXIS_COUNT as i32).contains(&axis) {
            return 0.0;
        }
        self.pads
            .get(&pad)
            .map(|p| p.axes[axis as usize])
            .unwrap_or(0.0)
    }

    pub fn drain_events_packed(&mut self) -> Vec<u8> {
        let count = self.queue.len() as u32;
        let mut out = Vec::with_capacity(4 + self.queue.len() * 16);
        out.extend_from_slice(&count.to_le_bytes());
        while let Some(ev) = self.queue.pop_front() {
            pack_event(&mut out, &ev);
        }
        out
    }
}

pub fn apply_deadzone(value: f32, zone: f32) -> f32 {
    if value.abs() < zone {
        0.0
    } else {
        value.clamp(-1.0, 1.0)
    }
}

fn push_str(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(b);
}

fn pack_event(out: &mut Vec<u8>, ev: &InputEvent) {
    match ev {
        InputEvent::PointerDown {
            x,
            y,
            button,
            pointer_id,
        } => {
            out.push(TAG_POINTER_DOWN);
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
            out.extend_from_slice(&button.to_le_bytes());
            out.extend_from_slice(&pointer_id.to_le_bytes());
        }
        InputEvent::PointerUp {
            x,
            y,
            button,
            pointer_id,
        } => {
            out.push(TAG_POINTER_UP);
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
            out.extend_from_slice(&button.to_le_bytes());
            out.extend_from_slice(&pointer_id.to_le_bytes());
        }
        InputEvent::PointerMove { x, y, pointer_id } => {
            out.push(TAG_POINTER_MOVE);
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
            out.extend_from_slice(&pointer_id.to_le_bytes());
        }
        InputEvent::PointerEnter { x, y, pointer_id } => {
            out.push(TAG_POINTER_ENTER);
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
            out.extend_from_slice(&pointer_id.to_le_bytes());
        }
        InputEvent::PointerLeave { x, y, pointer_id } => {
            out.push(TAG_POINTER_LEAVE);
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
            out.extend_from_slice(&pointer_id.to_le_bytes());
        }
        InputEvent::PointerCancel { pointer_id } => {
            out.push(TAG_POINTER_CANCEL);
            out.extend_from_slice(&pointer_id.to_le_bytes());
        }
        InputEvent::Wheel { dx, dy, x, y } => {
            out.push(TAG_WHEEL);
            out.extend_from_slice(&dx.to_le_bytes());
            out.extend_from_slice(&dy.to_le_bytes());
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
        }
        InputEvent::KeyDown { code, key, repeat } => {
            out.push(TAG_KEY_DOWN);
            push_str(out, code);
            push_str(out, key);
            out.push(u8::from(*repeat));
        }
        InputEvent::KeyUp { code, key } => {
            out.push(TAG_KEY_UP);
            push_str(out, code);
            push_str(out, key);
        }
        InputEvent::TextInput { text } => {
            out.push(TAG_TEXT_INPUT);
            push_str(out, text);
        }
        InputEvent::Resize { width, height } => {
            out.push(TAG_RESIZE);
            out.extend_from_slice(&width.to_le_bytes());
            out.extend_from_slice(&height.to_le_bytes());
        }
        InputEvent::ScaleFactor { scale } => {
            out.push(TAG_SCALE_FACTOR);
            out.extend_from_slice(&scale.to_le_bytes());
        }
        InputEvent::Focus => out.push(TAG_FOCUS),
        InputEvent::Blur => out.push(TAG_BLUR),
        InputEvent::Close => out.push(TAG_CLOSE),
        InputEvent::GamepadConnected { pad } => {
            out.push(TAG_GAMEPAD_CONNECTED);
            out.extend_from_slice(&pad.to_le_bytes());
        }
        InputEvent::GamepadDisconnected { pad } => {
            out.push(TAG_GAMEPAD_DISCONNECTED);
            out.extend_from_slice(&pad.to_le_bytes());
        }
        InputEvent::GamepadButtonDown { pad, button } => {
            out.push(TAG_GAMEPAD_BUTTON_DOWN);
            out.extend_from_slice(&pad.to_le_bytes());
            out.push(*button);
        }
        InputEvent::GamepadButtonUp { pad, button } => {
            out.push(TAG_GAMEPAD_BUTTON_UP);
            out.extend_from_slice(&pad.to_le_bytes());
            out.push(*button);
        }
        InputEvent::GamepadAxis { pad, axis, value } => {
            out.push(TAG_GAMEPAD_AXIS);
            out.extend_from_slice(&pad.to_le_bytes());
            out.push(*axis);
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_gamepad_button_events() {
        let mut input = InputState::default();
        input.gamepad_connected(0);
        input.gamepad_button_down(0, BTN_SOUTH);
        let bytes = input.drain_events_packed();
        assert!(bytes.len() >= 4);
        let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(count, 2);
        assert_eq!(bytes[4], TAG_GAMEPAD_CONNECTED);
        assert_eq!(bytes[9], TAG_GAMEPAD_BUTTON_DOWN);
        assert_eq!(bytes[14], BTN_SOUTH);
    }

    #[test]
    fn packs_gamepad_axis_events_on_change() {
        let mut input = InputState::default();
        input.gamepad_set_axis(0, AXIS_LEFT_STICK_X, 0.5);
        input.gamepad_set_axis(0, AXIS_LEFT_STICK_X, 0.5);
        let bytes = input.drain_events_packed();
        let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(count, 1);
        assert_eq!(bytes[4], TAG_GAMEPAD_AXIS);
        assert_eq!(bytes[9], AXIS_LEFT_STICK_X as u8);
    }

    #[test]
    fn packs_multiple_pointers() {
        let mut input = InputState::default();
        input.pointer_down(1.0, 2.0, 0, 7);
        input.pointer_down(3.0, 4.0, 0, 8);
        let bytes = input.pack_pointers_and_clear_deltas();
        let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(count, 2);
        let id0 = i32::from_le_bytes([bytes[32], bytes[33], bytes[34], bytes[35]]);
        let id1 = i32::from_le_bytes([bytes[64], bytes[65], bytes[66], bytes[67]]);
        assert_eq!((id0, id1), (7, 8));
    }

    #[test]
    fn stick_deadzone_zeros_small_values() {
        assert_eq!(apply_deadzone(0.1, STICK_DEADZONE), 0.0);
        assert!(apply_deadzone(0.5, STICK_DEADZONE) > 0.0);
    }
}
