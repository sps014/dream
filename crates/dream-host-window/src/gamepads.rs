use crate::wire::{Wire, kind};
use sdl3::GamepadSubsystem;
use sdl3::gamepad::{Axis, Button, Gamepad};
use sdl3::joystick::JoystickId;
use std::collections::BTreeMap;

const BUTTONS: [Button; 15] = [
    Button::South,
    Button::East,
    Button::West,
    Button::North,
    Button::LeftShoulder,
    Button::RightShoulder,
    Button::Back,
    Button::Start,
    Button::Guide,
    Button::LeftStick,
    Button::RightStick,
    Button::DPadUp,
    Button::DPadRight,
    Button::DPadDown,
    Button::DPadLeft,
];
const AXES: [Axis; 6] = [
    Axis::LeftX,
    Axis::LeftY,
    Axis::RightX,
    Axis::RightY,
    Axis::TriggerLeft,
    Axis::TriggerRight,
];

pub(crate) struct Pads {
    devices: BTreeMap<usize, (JoystickId, Gamepad)>,
    subsystem: GamepadSubsystem,
}

impl Pads {
    pub fn new(subsystem: GamepadSubsystem) -> Self {
        Self {
            devices: BTreeMap::new(),
            subsystem,
        }
    }

    pub fn update(&mut self) {
        self.devices.retain(|_, (_, gamepad)| gamepad.connected());
        for id in self.subsystem.gamepads().unwrap_or_default() {
            if self.devices.values().any(|(known, _)| *known == id) {
                continue;
            }
            let Some(slot) = (0..16).find(|slot| !self.devices.contains_key(slot)) else {
                break;
            };
            if let Ok(gamepad) = self.subsystem.open(id) {
                self.devices.insert(slot, (id, gamepad));
            }
        }
    }

    pub fn sample(&self, previous: &mut [Option<JoystickId>; 16], wire: &mut Wire) {
        for (slot, last) in previous.iter_mut().enumerate() {
            let current = self.devices.get(&slot);
            let id = current.map(|(id, _)| *id);
            if *last != id {
                if last.is_some() {
                    wire.push(kind::GAMEPAD_DISCONNECTED, &[slot as f32]);
                }
                if let Some((_, gamepad)) = current {
                    wire.push_text(
                        kind::GAMEPAD_CONNECTED,
                        &[slot as f32],
                        &gamepad.name().unwrap_or_default(),
                    );
                }
                *last = id;
            }
            if let Some((_, gamepad)) = current {
                let buttons = BUTTONS
                    .iter()
                    .enumerate()
                    .fold(0u32, |bits, (index, button)| {
                        bits | (u32::from(gamepad.button(*button)) << index)
                    });
                let mut payload = [0.0; 8];
                payload[0] = slot as f32;
                payload[1] = buttons as f32;
                for (index, axis) in AXES.iter().enumerate() {
                    payload[index + 2] = normalized_axis(gamepad.axis(*axis), index >= 4);
                }
                wire.push(kind::GAMEPAD_SAMPLE, &payload);
            }
        }
    }

    pub fn rumble(&mut self, slot: i32, low: f32, high: f32, ms: i32) -> bool {
        if ms <= 0 || !low.is_finite() || !high.is_finite() {
            return false;
        }
        let Some((_, gamepad)) = usize::try_from(slot)
            .ok()
            .and_then(|slot| self.devices.get_mut(&slot))
        else {
            return false;
        };
        gamepad
            .set_rumble(
                (low.clamp(0.0, 1.0) * u16::MAX as f32) as u16,
                (high.clamp(0.0, 1.0) * u16::MAX as f32) as u16,
                ms as u32,
            )
            .is_ok()
    }
}

fn normalized_axis(value: i16, trigger: bool) -> f32 {
    if trigger {
        value.max(0) as f32 / i16::MAX as f32
    } else if value < 0 {
        value as f32 / 32768.0
    } else {
        value as f32 / i16::MAX as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn axes_match_dream_ranges() {
        assert_eq!(normalized_axis(i16::MIN, false), -1.0);
        assert_eq!(normalized_axis(i16::MAX, false), 1.0);
        assert_eq!(normalized_axis(0, false), 0.0);
        assert_eq!(normalized_axis(i16::MIN, true), 0.0);
        assert_eq!(normalized_axis(i16::MAX, true), 1.0);
    }
}
