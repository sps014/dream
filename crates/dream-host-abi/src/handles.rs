//! Generational handle tables: guest-visible positive `i32` ids that are recycled without ever
//! running out, so a long-running process never exhausts its id space.
//!
//! A handle packs `(generation << 23) | slot`. Generations run 1..=255, so a handle is always
//! positive and 0 keeps meaning "no handle". Freeing a slot bumps its generation, so a stale
//! handle misses instead of naming the slot's next occupant. Free slots are reused oldest first,
//! so a slot comes back only after every other free slot has, and its generation wraps only after
//! 255 trips through the whole free queue.

use std::collections::VecDeque;

const SLOT_BITS: u32 = 23;
const SLOT_MASK: u32 = (1 << SLOT_BITS) - 1;
/// The most handles one table holds at once.
pub const MAX_LIVE: usize = SLOT_MASK as usize;

struct Slot<T> {
    generation: u8,
    value: Option<T>,
}

pub struct HandleTable<T> {
    slots: Vec<Slot<T>>,
    free: VecDeque<u32>,
    live: usize,
}

impl<T> Default for HandleTable<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> HandleTable<T> {
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: VecDeque::new(),
            live: 0,
        }
    }

    fn handle(slot: u32, generation: u8) -> i32 {
        ((u32::from(generation) << SLOT_BITS) | slot) as i32
    }

    fn locate(&self, handle: i32) -> Option<usize> {
        let raw = u32::try_from(handle).ok()?;
        let index = (raw & SLOT_MASK) as usize;
        let generation = (raw >> SLOT_BITS) as u8;
        let slot = self.slots.get(index)?;
        (slot.generation == generation && slot.value.is_some()).then_some(index)
    }

    /// Stores `value` and returns its handle, or `None` when `MAX_LIVE` handles are live.
    pub fn insert(&mut self, value: T) -> Option<i32> {
        let index = match self.free.pop_front() {
            Some(index) => index,
            None if self.slots.len() < MAX_LIVE => {
                self.slots.push(Slot {
                    generation: 1,
                    value: None,
                });
                (self.slots.len() - 1) as u32
            }
            None => return None,
        };
        let slot = &mut self.slots[index as usize];
        slot.value = Some(value);
        self.live += 1;
        Some(Self::handle(index, slot.generation))
    }

    pub fn get(&self, handle: i32) -> Option<&T> {
        let index = self.locate(handle)?;
        self.slots[index].value.as_ref()
    }

    pub fn get_mut(&mut self, handle: i32) -> Option<&mut T> {
        let index = self.locate(handle)?;
        self.slots[index].value.as_mut()
    }

    pub fn contains(&self, handle: i32) -> bool {
        self.locate(handle).is_some()
    }

    pub fn remove(&mut self, handle: i32) -> Option<T> {
        let index = self.locate(handle)?;
        let slot = &mut self.slots[index];
        let value = slot.value.take();
        slot.generation = if slot.generation == u8::MAX {
            1
        } else {
            slot.generation + 1
        };
        self.free.push_back(index as u32);
        self.live -= 1;
        value
    }

    /// Live handles.
    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Live handles and their values, in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (i32, &T)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.value
                .as_ref()
                .map(|value| (Self::handle(index as u32, slot.generation), value))
        })
    }

    /// Removes every live value, in slot order.
    pub fn drain(&mut self) -> Vec<T> {
        let handles: Vec<i32> = self.iter().map(|(handle, _)| handle).collect();
        handles
            .into_iter()
            .filter_map(|handle| self.remove(handle))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_are_positive_and_stale_handles_miss() {
        let mut table = HandleTable::new();
        let first = table.insert("a").unwrap();
        assert!(first > 0);
        assert_eq!(table.get(first), Some(&"a"));
        assert_eq!(table.remove(first), Some("a"));
        assert_eq!(table.get(first), None);
        assert_eq!(table.remove(first), None);
        let second = table.insert("b").unwrap();
        assert_ne!(first, second, "a reused slot carries a new generation");
        assert_eq!(table.get(first), None);
        assert_eq!(table.get(second), Some(&"b"));
        assert_eq!(table.get(0), None);
        assert_eq!(table.get(-1), None);
    }

    #[test]
    fn free_slots_are_reused_oldest_first() {
        let mut table = HandleTable::new();
        let handles: Vec<i32> = (0..3).map(|i| table.insert(i).unwrap()).collect();
        table.remove(handles[1]);
        table.remove(handles[0]);
        table.remove(handles[2]);
        let slot = |h: i32| h as u32 & SLOT_MASK;
        let reused: Vec<u32> = (0..3).map(|i| slot(table.insert(i).unwrap())).collect();
        assert_eq!(reused, vec![1, 0, 2]);
    }

    #[test]
    fn generations_wrap_to_one_never_zero() {
        let mut table = HandleTable::new();
        let mut seen = Vec::new();
        for _ in 0..600 {
            let handle = table.insert(()).unwrap();
            assert!(handle > 0);
            seen.push((handle as u32) >> SLOT_BITS);
            table.remove(handle);
        }
        assert!(seen.iter().all(|&g| (1..=255).contains(&g)));
        assert_eq!(seen[254], 255);
        assert_eq!(seen[255], 1);
    }

    #[test]
    fn ten_million_cycles_exceed_the_old_ceiling_without_exhausting() {
        let mut table = HandleTable::new();
        let keep: Vec<i32> = (0..64).map(|i| table.insert(i).unwrap()).collect();
        for i in 0..10_000_000u32 {
            let handle = table.insert(i).expect("ids are recycled");
            assert_eq!(table.remove(handle), Some(i));
        }
        assert_eq!(table.len(), keep.len());
        for (i, handle) in keep.into_iter().enumerate() {
            assert_eq!(table.get(handle), Some(&(i as u32)));
        }
    }

    #[test]
    fn insert_fails_only_when_every_slot_is_live() {
        let mut table = HandleTable::new();
        for _ in 0..MAX_LIVE {
            table.insert(()).unwrap();
        }
        assert_eq!(table.insert(()), None);
        let any = table.iter().next().unwrap().0;
        table.remove(any);
        assert!(table.insert(()).is_some());
    }
}
