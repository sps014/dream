//! GLFW key tokens to `system.window.KeyCode` values. GLFW names keys by their US-layout
//! position, which is exactly the physical key the Dream enum describes.

use glfw::Key;

/// The `KeyCode` value for `key`; `0` (`KeyCode.Unknown`) when the key has no Dream name.
pub fn key_code(key: Key) -> u8 {
    let raw = key as i32;
    match raw {
        65..=90 => (raw - 65 + 1) as u8,
        48..=57 => (raw - 48 + 27) as u8,
        290..=313 => (raw - 290 + 37) as u8,
        320..=329 => (raw - 320 + 93) as u8,
        _ => named(key),
    }
}

fn named(key: Key) -> u8 {
    match key {
        Key::Escape => 61,
        Key::Enter => 62,
        Key::Tab => 63,
        Key::Backspace => 64,
        Key::Space => 65,
        Key::Insert => 66,
        Key::Delete => 67,
        Key::Right => 68,
        Key::Left => 69,
        Key::Down => 70,
        Key::Up => 71,
        Key::PageUp => 72,
        Key::PageDown => 73,
        Key::Home => 74,
        Key::End => 75,
        Key::CapsLock => 76,
        Key::ScrollLock => 77,
        Key::NumLock => 78,
        Key::PrintScreen => 79,
        Key::Pause => 80,
        Key::Minus => 81,
        Key::Equal => 82,
        Key::LeftBracket => 83,
        Key::RightBracket => 84,
        Key::Backslash => 85,
        Key::Semicolon => 86,
        Key::Apostrophe => 87,
        Key::GraveAccent => 88,
        Key::Comma => 89,
        Key::Period => 90,
        Key::Slash => 91,
        Key::World1 => 92,
        Key::KpDecimal => 103,
        Key::KpDivide => 104,
        Key::KpMultiply => 105,
        Key::KpSubtract => 106,
        Key::KpAdd => 107,
        Key::KpEnter => 108,
        Key::KpEqual => 109,
        Key::LeftShift => 110,
        Key::LeftControl => 111,
        Key::LeftAlt => 112,
        Key::LeftSuper => 113,
        Key::RightShift => 114,
        Key::RightControl => 115,
        Key::RightAlt => 116,
        Key::RightSuper => 117,
        Key::Menu => 118,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_line_up_with_the_dream_enum() {
        assert_eq!(key_code(Key::A), 1);
        assert_eq!(key_code(Key::D), 4);
        assert_eq!(key_code(Key::Z), 26);
        assert_eq!(key_code(Key::Num0), 27);
        assert_eq!(key_code(Key::Num9), 36);
        assert_eq!(key_code(Key::F1), 37);
        assert_eq!(key_code(Key::F11), 47);
        assert_eq!(key_code(Key::F24), 60);
        assert_eq!(key_code(Key::F25), 0);
        assert_eq!(key_code(Key::Kp0), 93);
        assert_eq!(key_code(Key::Kp9), 102);
        assert_eq!(key_code(Key::Escape), 61);
        assert_eq!(key_code(Key::Menu), 118);
        assert_eq!(key_code(Key::World2), 0);
        assert_eq!(key_code(Key::Unknown), 0);
    }
}
