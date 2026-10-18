//! Guest handles: positive ints, unique across every resource kind for the life of the process.
//! `0` is the failure value, so it is never issued; released handles are never reissued, so a
//! stale handle can only miss, never alias a newer resource.

use std::collections::HashMap;

pub struct Handles<T> {
    next: i32,
    entries: HashMap<i32, T>,
}

impl<T> Default for Handles<T> {
    fn default() -> Self {
        Self {
            next: 1,
            entries: HashMap::new(),
        }
    }
}

impl<T> Handles<T> {
    /// `None` once the positive ints are exhausted.
    pub fn insert(&mut self, value: T) -> Option<i32> {
        if self.next <= 0 {
            return None;
        }
        let handle = self.next;
        self.next = self.next.checked_add(1).unwrap_or(0);
        self.entries.insert(handle, value);
        Some(handle)
    }

    pub fn get(&self, handle: i32) -> Option<&T> {
        self.entries.get(&handle)
    }

    pub fn get_mut(&mut self, handle: i32) -> Option<&mut T> {
        self.entries.get_mut(&handle)
    }

    pub fn remove(&mut self, handle: i32) -> Option<T> {
        self.entries.remove(&handle)
    }

    #[cfg(test)]
    fn set_next(&mut self, next: i32) {
        self.next = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_are_positive_unique_and_never_reused() {
        let mut table = Handles::default();
        let a = table.insert("a").unwrap();
        let b = table.insert("b").unwrap();
        assert!(a > 0 && b > 0 && a != b);
        assert_eq!(table.remove(a), Some("a"));
        assert_eq!(table.get(a), None);
        let c = table.insert("c").unwrap();
        assert!(c != a && c != b);
        assert_eq!(table.get(b), Some(&"b"));
    }

    #[test]
    fn zero_and_negative_handles_never_resolve() {
        let mut table = Handles::default();
        table.insert(1).unwrap();
        assert!(table.get(0).is_none());
        assert!(table.get(-1).is_none());
        assert!(table.remove(0).is_none());
    }

    #[test]
    fn exhaustion_fails_instead_of_wrapping() {
        let mut table = Handles::default();
        table.set_next(i32::MAX);
        assert_eq!(table.insert(1), Some(i32::MAX));
        assert_eq!(table.insert(2), None);
        assert_eq!(table.insert(3), None);
    }
}
