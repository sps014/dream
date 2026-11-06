//! Current values behind `Application.is_session_locked`, `are_displays_on`, `is_active`,
//! `power_source`, and `theme`. -1 means the platform has not reported the fact.

use crate::events;
use crate::queue;
use std::sync::atomic::{AtomicI32, Ordering};

pub const SESSION_LOCKED: usize = 0;
pub const DISPLAYS_ON: usize = 1;
pub const ACTIVE: usize = 2;
pub const POWER_SOURCE: usize = 3;
pub const THEME: usize = 4;

pub const POWER_AC: i32 = 1;
pub const POWER_BATTERY: i32 = 2;
pub const THEME_LIGHT: i32 = 0;
pub const THEME_DARK: i32 = 1;
#[cfg(windows)]
pub const THEME_HIGH_CONTRAST: i32 = 2;

static VALUES: [AtomicI32; 5] = [
    AtomicI32::new(-1),
    AtomicI32::new(-1),
    AtomicI32::new(-1),
    AtomicI32::new(-1),
    AtomicI32::new(-1),
];

const EVENT_FOR_FACT: [i32; 5] = [
    events::SESSION_LOCK,
    events::DISPLAY_POWER,
    events::ACTIVE,
    events::POWER_SOURCE,
    events::THEME,
];

pub fn get(fact: i32) -> i32 {
    usize::try_from(fact)
        .ok()
        .and_then(|index| VALUES.get(index))
        .map_or(-1, |value| value.load(Ordering::SeqCst))
}

/// Records a value read at startup without reporting it as a change.
pub fn seed(fact: usize, value: i32) {
    VALUES[fact].store(value, Ordering::SeqCst);
}

/// Records a new value and posts the matching event when it changed.
pub fn report(fact: usize, value: i32) {
    if VALUES[fact].swap(value, Ordering::SeqCst) != value {
        queue::post(EVENT_FOR_FACT[fact], value);
    }
}

/// Window hosts report whether any window of the program has focus.
pub extern "C" fn on_activity(active: i32) {
    report(ACTIVE, i32::from(active != 0));
}
