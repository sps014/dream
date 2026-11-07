//! Event kinds and the coalescing queue, shared with `system/app/app.dream` and
//! `runtime/src/hosts/app.js`.

use std::collections::VecDeque;

pub const INTERRUPT: i32 = 1;
pub const TERMINATE: i32 = 2;
/// Value 0 asks whether the session may end and waits for a reply; 1 is the final notice.
#[cfg(any(target_os = "macos", windows))]
pub const SESSION_ENDING: i32 = 3;
#[cfg(any(target_os = "macos", windows))]
pub const SLEEPING: i32 = 4;
#[cfg(any(target_os = "macos", windows))]
pub const WOKE: i32 = 5;
pub const DISPLAY_POWER: i32 = 6;
pub const SESSION_LOCK: i32 = 7;
pub const ACTIVE: i32 = 8;
pub const POWER_SOURCE: i32 = 9;
pub const THEME: i32 = 10;
pub const LOCALE: i32 = 11;
pub const LOW_MEMORY: i32 = 12;

#[derive(Default)]
pub struct Events {
    items: VecDeque<(i32, i32)>,
}

impl Events {
    pub const fn new() -> Self {
        Self {
            items: VecDeque::new(),
        }
    }

    /// Queues an event unless an equivalent one is already waiting. Returns whether it was
    /// queued.
    pub fn push(&mut self, kind: i32, value: i32) -> bool {
        let duplicate = match kind {
            LOCALE | LOW_MEMORY => self.items.iter().any(|&(k, _)| k == kind),
            THEME | POWER_SOURCE => self.items.contains(&(kind, value)),
            _ => false,
        };
        if !duplicate {
            self.items.push_back((kind, value));
        }
        !duplicate
    }

    pub fn pop(&mut self) -> Option<(i32, i32)> {
        self.items.pop_front()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_low_memory_and_locale_collapse() {
        let mut events = Events::new();
        assert!(events.push(LOW_MEMORY, 0));
        assert!(!events.push(LOW_MEMORY, 0));
        assert!(events.push(LOCALE, 0));
        assert!(!events.push(LOCALE, 0));
        assert_eq!(events.pop(), Some((LOW_MEMORY, 0)));
        assert!(events.push(LOW_MEMORY, 0));
    }

    #[test]
    fn only_identical_theme_and_power_events_collapse() {
        let mut events = Events::new();
        assert!(events.push(THEME, 1));
        assert!(!events.push(THEME, 1));
        assert!(events.push(THEME, 0));
        assert!(events.push(POWER_SOURCE, 2));
        assert!(!events.push(POWER_SOURCE, 2));
        assert!(events.push(SESSION_LOCK, 1));
        assert!(events.push(SESSION_LOCK, 1));
        let drained: Vec<_> = std::iter::from_fn(|| events.pop()).collect();
        assert_eq!(
            drained,
            vec![
                (THEME, 1),
                (THEME, 0),
                (POWER_SOURCE, 2),
                (SESSION_LOCK, 1),
                (SESSION_LOCK, 1)
            ]
        );
    }
}
