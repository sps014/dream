//! SDL physical scancodes mapped to Dream KeyCode order, independently of keyboard layout.

use sdl3::keyboard::Scancode;

/// The `KeyCode` value for `key`; `0` (`KeyCode.Unknown`) when the key has no Dream name.
pub fn key_code(key: Scancode) -> u8 {
    let raw = key as i32;
    match raw {
        4..=29 => (raw - 4 + 1) as u8,
        30..=38 => (raw - 30 + 28) as u8,
        39 => 27,
        58..=69 => (raw - 58 + 37) as u8,
        104..=115 => (raw - 104 + 49) as u8,
        89..=97 => (raw - 89 + 94) as u8,
        98 => 93,
        _ => named(key),
    }
}

fn named(key: Scancode) -> u8 {
    match key {
        Scancode::Escape => 61,
        Scancode::Return => 62,
        Scancode::Tab => 63,
        Scancode::Backspace => 64,
        Scancode::Space => 65,
        Scancode::Insert => 66,
        Scancode::Delete => 67,
        Scancode::Right => 68,
        Scancode::Left => 69,
        Scancode::Down => 70,
        Scancode::Up => 71,
        Scancode::PageUp => 72,
        Scancode::PageDown => 73,
        Scancode::Home => 74,
        Scancode::End => 75,
        Scancode::CapsLock => 76,
        Scancode::ScrollLock => 77,
        Scancode::NumLockClear => 78,
        Scancode::PrintScreen => 79,
        Scancode::Pause => 80,
        Scancode::Minus => 81,
        Scancode::Equals => 82,
        Scancode::LeftBracket => 83,
        Scancode::RightBracket => 84,
        Scancode::Backslash => 85,
        Scancode::Semicolon => 86,
        Scancode::Apostrophe => 87,
        Scancode::Grave => 88,
        Scancode::Comma => 89,
        Scancode::Period => 90,
        Scancode::Slash => 91,
        Scancode::NonUsBackslash => 92,
        Scancode::KpPeriod => 103,
        Scancode::KpDivide => 104,
        Scancode::KpMultiply => 105,
        Scancode::KpMinus => 106,
        Scancode::KpPlus => 107,
        Scancode::KpEnter => 108,
        Scancode::KpEquals => 109,
        Scancode::LShift => 110,
        Scancode::LCtrl => 111,
        Scancode::LAlt => 112,
        Scancode::LGui => 113,
        Scancode::RShift => 114,
        Scancode::RCtrl => 115,
        Scancode::RAlt => 116,
        Scancode::RGui => 117,
        Scancode::Application => 118,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scancodes_keep_dream_key_order() {
        for (key, expected) in [
            (Scancode::A, 1),
            (Scancode::Z, 26),
            (Scancode::_0, 27),
            (Scancode::_9, 36),
            (Scancode::F1, 37),
            (Scancode::F12, 48),
            (Scancode::F13, 49),
            (Scancode::F24, 60),
            (Scancode::Kp0, 93),
            (Scancode::Kp9, 102),
            (Scancode::Escape, 61),
            (Scancode::Application, 118),
            (Scancode::Mute, 0),
        ] {
            assert_eq!(key_code(key), expected, "{key:?}");
        }
    }
}
