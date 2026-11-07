use crate::metrics::Metrics;
use crate::wire::{Wire, kind};
use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::{Keycode, Mod};
use sdl3::mouse::{MouseButton, MouseWheelDirection};

#[derive(Default)]
pub(crate) struct Input {
    pub wire: Wire,
    pub focused: bool,
    pub hidden: bool,
    pub captured: bool,
    pub cursor: (f32, f32),
    touches: Vec<((u64, u64), i32)>,
}

impl Input {
    pub fn cancel_touches(&mut self) {
        for (_, id) in self.touches.drain(..) {
            self.wire.push(kind::TOUCH_CANCEL, &[id as f32]);
        }
    }

    pub fn occlusion(&mut self, hidden: bool) {
        if self.hidden != hidden {
            self.hidden = hidden;
            if hidden {
                self.cancel_touches();
            }
            self.wire.push(kind::OCCLUDED, &[u8::from(hidden) as f32]);
        }
    }

    pub fn encode(&mut self, event: &Event, metrics: Metrics) {
        match event {
            Event::Window { win_event, .. } => match win_event {
                WindowEvent::Moved(x, y) => self.wire.push(kind::MOVED, &[*x as f32, *y as f32]),
                WindowEvent::FocusGained => {
                    self.focused = true;
                    self.wire.push(kind::FOCUSED, &[]);
                }
                WindowEvent::FocusLost => {
                    self.cancel_touches();
                    self.focused = false;
                    self.wire.push(kind::UNFOCUSED, &[]);
                }
                WindowEvent::CloseRequested => self.wire.push(kind::CLOSE_REQUESTED, &[]),
                WindowEvent::Minimized => {
                    self.cancel_touches();
                    self.wire.push(kind::MINIMIZED, &[]);
                }
                WindowEvent::Restored => self.wire.push(kind::RESTORED, &[]),
                _ => {}
            },
            Event::KeyDown {
                keycode,
                scancode,
                keymod,
                repeat,
                ..
            }
            | Event::KeyUp {
                keycode,
                scancode,
                keymod,
                repeat,
                ..
            } => {
                let down = matches!(event, Event::KeyDown { .. });
                let code = scancode.map_or(0, crate::keys::key_code);
                self.wire.push_text(
                    if down { kind::KEY_DOWN } else { kind::KEY_UP },
                    &[
                        f32::from(code),
                        modifiers(*keymod) as f32,
                        u8::from(down && *repeat) as f32,
                    ],
                    &key_name(*keycode),
                );
            }
            Event::TextInput { text, .. } => self.wire.push_text(kind::TEXT, &[], text),
            Event::MouseMotion {
                x, y, xrel, yrel, ..
            } => {
                self.cursor = if self.captured {
                    (
                        self.cursor.0 + xrel / metrics.coordinates,
                        self.cursor.1 + yrel / metrics.coordinates,
                    )
                } else {
                    (x / metrics.coordinates, y / metrics.coordinates)
                };
                self.wire
                    .push(kind::MOUSE_MOVE, &[self.cursor.0, self.cursor.1]);
            }
            Event::MouseButtonDown {
                mouse_btn, x, y, ..
            }
            | Event::MouseButtonUp {
                mouse_btn, x, y, ..
            } => {
                if let Some(button) = mouse_button(*mouse_btn) {
                    // A click can arrive without a preceding motion event (notably on focus).
                    if !self.captured {
                        self.cursor = (x / metrics.coordinates, y / metrics.coordinates);
                        self.wire
                            .push(kind::MOUSE_MOVE, &[self.cursor.0, self.cursor.1]);
                    }
                    self.wire.push(
                        if matches!(event, Event::MouseButtonDown { .. }) {
                            kind::MOUSE_DOWN
                        } else {
                            kind::MOUSE_UP
                        },
                        &[button as f32],
                    );
                }
            }
            Event::MouseWheel {
                x, y, direction, ..
            } => {
                let sign = if *direction == MouseWheelDirection::Flipped {
                    -1.0
                } else {
                    1.0
                };
                self.wire.push(kind::SCROLL, &[x * sign, y * sign]);
            }
            Event::DropFile { filename, .. } => {
                self.wire.push_text(kind::FILE_DROPPED, &[], filename)
            }
            Event::FingerDown {
                touch_id,
                finger_id,
                x,
                y,
                ..
            }
            | Event::FingerMotion {
                touch_id,
                finger_id,
                x,
                y,
                ..
            }
            | Event::FingerUp {
                touch_id,
                finger_id,
                x,
                y,
                ..
            }
            | Event::FingerCanceled {
                touch_id,
                finger_id,
                x,
                y,
                ..
            } => {
                self.touch(event, (*touch_id, *finger_id), (*x, *y), metrics);
            }
            _ => {}
        }
    }

    fn touch(&mut self, event: &Event, token: (u64, u64), point: (f32, f32), metrics: Metrics) {
        if !matches!(event, Event::FingerCanceled { .. })
            && (!point.0.is_finite() || !point.1.is_finite())
        {
            return;
        }
        if matches!(event, Event::FingerDown { .. }) {
            if self.touches.iter().any(|(key, _)| *key == token) {
                return;
            }
            // SDL ids are 64-bit; compact ids survive the f32 event wire without rounding.
            let Some(id) =
                (0..=16_777_215).find(|id| !self.touches.iter().any(|(_, used)| used == id))
            else {
                return;
            };
            self.touches.push((token, id));
        }
        let Some(index) = self.touches.iter().position(|(key, _)| *key == token) else {
            return;
        };
        let id = self.touches[index].1 as f32;
        let record = match event {
            Event::FingerDown { .. } => kind::TOUCH_DOWN,
            Event::FingerMotion { .. } => kind::TOUCH_MOVE,
            Event::FingerUp { .. } => kind::TOUCH_UP,
            _ => kind::TOUCH_CANCEL,
        };
        if record == kind::TOUCH_CANCEL {
            self.wire.push(record, &[id]);
        } else {
            self.wire.push(
                record,
                &[id, point.0 * metrics.width, point.1 * metrics.height],
            );
        }
        if matches!(record, kind::TOUCH_UP | kind::TOUCH_CANCEL) {
            self.touches.remove(index);
        }
    }
}

fn mouse_button(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Right => Some(1),
        MouseButton::Middle => Some(2),
        MouseButton::X1 => Some(3),
        MouseButton::X2 => Some(4),
        _ => None,
    }
}

fn key_name(key: Option<Keycode>) -> String {
    key.and_then(|key| char::from_u32(key as u32))
        .filter(|ch| !ch.is_control())
        .map_or_else(String::new, |ch| ch.to_string())
}

fn modifiers(value: Mod) -> u8 {
    let flags = [
        Mod::LSHIFTMOD | Mod::RSHIFTMOD,
        Mod::LCTRLMOD | Mod::RCTRLMOD,
        Mod::LALTMOD | Mod::RALTMOD,
        Mod::LGUIMOD | Mod::RGUIMOD,
        Mod::CAPSMOD,
        Mod::NUMMOD,
    ];
    flags.iter().enumerate().fold(0, |bits, (index, flag)| {
        bits | (u8::from(value.intersects(*flag)) << index)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> Metrics {
        Metrics {
            width: 320.0,
            height: 200.0,
            pixels: (640, 400),
            scale: 2.0,
            coordinates: 2.0,
            insets: [0.0; 4],
        }
    }
    fn touch(finger_id: u64) -> Event {
        Event::FingerDown {
            timestamp: 0,
            touch_id: u64::MAX,
            finger_id,
            x: 0.25,
            y: 0.5,
            dx: 0.0,
            dy: 0.0,
            pressure: 1.0,
            window_id: 1,
        }
    }
    #[test]
    fn touch_ids_survive_float_wire_and_cancel_on_focus_loss() {
        let mut input = Input::default();
        input.encode(&touch(u64::MAX), metrics());
        input.encode(&touch(u64::MAX - 1), metrics());
        input.encode(&touch(u64::MAX), metrics());
        input.encode(
            &Event::Window {
                timestamp: 0,
                window_id: 1,
                win_event: WindowEvent::FocusLost,
            },
            metrics(),
        );
        assert_eq!(
            input.wire.values(),
            &[
                21.0, 3.0, 0.0, 80.0, 100.0, 21.0, 3.0, 1.0, 80.0, 100.0, 24.0, 1.0, 0.0, 24.0,
                1.0, 1.0, 4.0, 0.0
            ]
        );
        assert!(input.touches.is_empty());
    }
    #[test]
    fn cancellation_does_not_require_a_valid_position() {
        let mut input = Input::default();
        input.encode(&touch(10), metrics());
        input.encode(
            &Event::FingerCanceled {
                timestamp: 0,
                touch_id: u64::MAX,
                finger_id: 10,
                x: f32::NAN,
                y: f32::NAN,
                dx: 0.0,
                dy: 0.0,
                pressure: 0.0,
                window_id: 1,
            },
            metrics(),
        );
        assert!(input.touches.is_empty());
        assert!(input.wire.values().ends_with(&[24.0, 1.0, 0.0]));
    }
    #[test]
    fn click_coordinates_and_captured_motion_use_logical_units() {
        let mut input = Input::default();
        input.encode(
            &Event::MouseButtonDown {
                timestamp: 0,
                window_id: 1,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x: 80.0,
                y: 120.0,
            },
            metrics(),
        );
        assert_eq!(input.wire.values(), &[9.0, 2.0, 40.0, 60.0, 10.0, 1.0, 0.0]);
        input.captured = true;
        input.encode(
            &Event::MouseMotion {
                timestamp: 0,
                window_id: 1,
                which: 0,
                mousestate: sdl3::mouse::MouseState::from_sdl_state(0),
                x: 1.0,
                y: 1.0,
                xrel: 10.0,
                yrel: -20.0,
            },
            metrics(),
        );
        assert_eq!(input.cursor, (45.0, 50.0));
    }
    #[test]
    fn utf16_and_modifier_bits_match_the_shared_wire() {
        let mut input = Input::default();
        input.encode(
            &Event::TextInput {
                timestamp: 0,
                window_id: 1,
                text: "😀".into(),
            },
            metrics(),
        );
        assert_eq!(input.wire.values(), &[8.0, 2.0, 55357.0, 56832.0]);
        assert_eq!(modifiers(Mod::LSHIFTMOD | Mod::RCTRLMOD | Mod::CAPSMOD), 19);
    }
}
