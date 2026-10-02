//! Native gamepad host via gilrs. Pumps into each surface's `InputState`.

use super::input::{
    apply_deadzone, AXIS_LEFT_STICK_X, AXIS_LEFT_STICK_Y, AXIS_LEFT_TRIGGER, AXIS_RIGHT_STICK_X,
    AXIS_RIGHT_STICK_Y, AXIS_RIGHT_TRIGGER, BTN_DPAD_DOWN, BTN_DPAD_LEFT, BTN_DPAD_RIGHT,
    BTN_DPAD_UP, BTN_EAST, BTN_LEFT_SHOULDER, BTN_LEFT_STICK, BTN_LEFT_TRIGGER, BTN_NORTH,
    BTN_RIGHT_SHOULDER, BTN_RIGHT_STICK, BTN_RIGHT_TRIGGER, BTN_SELECT, BTN_SOUTH, BTN_START,
    BTN_UNKNOWN, BTN_WEST, STICK_DEADZONE,
};
use super::state::lock_state;
use gilrs::{Axis, Button, EventType, Gilrs};
use std::cell::RefCell;

thread_local! {
    static GILRS: RefCell<Option<Gilrs>> = const { RefCell::new(None) };
}

fn with_gilrs<R>(f: impl FnOnce(&mut Gilrs) -> R) -> Option<R> {
    GILRS.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Gilrs::new().ok();
        }
        slot.as_mut().map(f)
    })
}

fn map_button(button: Button) -> u8 {
    match button {
        Button::South => BTN_SOUTH,
        Button::East => BTN_EAST,
        Button::West => BTN_WEST,
        Button::North => BTN_NORTH,
        Button::DPadUp => BTN_DPAD_UP,
        Button::DPadDown => BTN_DPAD_DOWN,
        Button::DPadLeft => BTN_DPAD_LEFT,
        Button::DPadRight => BTN_DPAD_RIGHT,
        Button::LeftTrigger => BTN_LEFT_SHOULDER,
        Button::LeftTrigger2 => BTN_LEFT_TRIGGER,
        Button::RightTrigger => BTN_RIGHT_SHOULDER,
        Button::RightTrigger2 => BTN_RIGHT_TRIGGER,
        Button::LeftThumb => BTN_LEFT_STICK,
        Button::RightThumb => BTN_RIGHT_STICK,
        Button::Start => BTN_START,
        Button::Select => BTN_SELECT,
        _ => BTN_UNKNOWN,
    }
}

fn map_axis(axis: Axis) -> Option<usize> {
    match axis {
        Axis::LeftStickX => Some(AXIS_LEFT_STICK_X),
        Axis::LeftStickY => Some(AXIS_LEFT_STICK_Y),
        Axis::RightStickX => Some(AXIS_RIGHT_STICK_X),
        Axis::RightStickY => Some(AXIS_RIGHT_STICK_Y),
        Axis::LeftZ => Some(AXIS_LEFT_TRIGGER),
        Axis::RightZ => Some(AXIS_RIGHT_TRIGGER),
        _ => None,
    }
}

fn pad_id(id: gilrs::GamepadId) -> i32 {
    usize::from(id) as i32
}

fn apply_event(input: &mut super::input::InputState, pad: i32, event: &EventType) {
    match *event {
        EventType::Connected => input.gamepad_connected(pad),
        EventType::Disconnected => input.gamepad_disconnected(pad),
        EventType::ButtonPressed(button, _) => {
            let b = map_button(button);
            if b != BTN_UNKNOWN {
                input.gamepad_button_down(pad, b);
            }
        }
        EventType::ButtonReleased(button, _) => {
            let b = map_button(button);
            if b != BTN_UNKNOWN {
                input.gamepad_button_up(pad, b);
            }
        }
        EventType::AxisChanged(axis, value, _) => {
            if let Some(ax) = map_axis(axis) {
                let v = if ax <= AXIS_RIGHT_STICK_Y {
                    apply_deadzone(value, STICK_DEADZONE)
                } else {
                    ((value + 1.0) * 0.5).clamp(0.0, 1.0)
                };
                input.gamepad_set_axis(pad, ax, v);
            }
        }
        _ => {}
    }
}

/// Drain gilrs events into every live surface input queue / latch.
pub fn pump() {
    let (events, connected): (Vec<(gilrs::GamepadId, EventType)>, Vec<gilrs::GamepadId>) =
        match with_gilrs(|gilrs| {
            let mut events = Vec::new();
            while let Some(ev) = gilrs.next_event() {
                events.push((ev.id, ev.event));
            }
            let connected: Vec<_> = gilrs.gamepads().map(|(id, _)| id).collect();
            (events, connected)
        }) {
            Some(v) => v,
            None => return,
        };

    let mut st = lock_state();
    for (_sid, surface) in st.surfaces.iter_mut() {
        for &id in &connected {
            let pad = pad_id(id);
            if !surface.input.gamepad_is_connected(pad) {
                surface.input.gamepad_connected(pad);
            }
        }
        for &(id, ref event) in &events {
            apply_event(&mut surface.input, pad_id(id), event);
        }
    }
}
