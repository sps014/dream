//! Async/await frame layout shared by the C backend.
//!
//! An `async fun` compiles to a **constructor** (allocates a `Future` frame, stores params,
//! enqueues the first poll, returns the frame pointer) and a **poll** function (resumable state
//! machine between `await` points). Saved locals are laid out 8-byte aligned starting at
//! [`crate::abi::FutureLayout::slots`]. Field sizes differ per target (wasm32 vs native pointer
//! width), so callers supply them via the `local_bytes` closure.
//!
//! With `pack` enabled, locals whose lifetimes never overlap (per MIR liveness) share one slot:
//! the frame is canonical storage across suspends — scalars are re-read at poll entry and value
//! locals alias it by pointer — so disjoint live ranges can safely reuse the same bytes.

use super::MirFunction;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::HashMap;

/// Slot offsets for one async function's `Future` frame.
pub struct AsyncSlots {
    /// Local index → byte offset of its slot within the `Future` frame.
    pub offsets: HashMap<usize, i32>,
    /// Byte size of the whole frame (header + slots).
    pub frame_size: i32,
}

/// Lays out Future-frame slots, 8-byte aligned, starting at `slots_start`.
/// `local_bytes` returns `(size, is_value_type)` for a non-void local; void locals are omitted.
///
/// `pack` enables lifetime-overlap packing: locals with disjoint live ranges
/// (see [`crate::passes::rc::liveness::frame_interference`]) are assigned to a
/// shared slot sized for the largest member. Disabled under debug info, where
/// the frame-view debugger layout expects one named field per local. An optional
/// filter restricts which locals may share at all (by type); `None` allows all.
pub fn layout_async_slots(
    func: &MirFunction,
    interner: &TypeInterner,
    slots_start: i32,
    local_bytes: impl Fn(TypeId) -> (u32, bool),
    pack: Option<std::boxed::Box<dyn Fn(TypeId) -> bool + '_>>,
) -> AsyncSlots {
    let eligible: Vec<usize> = func
        .locals
        .iter()
        .enumerate()
        .filter(|(_, decl)| !matches!(interner.kind(decl.ty), TyKind::Void))
        .map(|(i, _)| i)
        .collect();
    let mut offsets: HashMap<usize, i32> = HashMap::new();

    let Some(packable) = pack else {
        let mut cursor = slots_start;
        for &i in &eligible {
            cursor = (cursor + 7) & !7;
            offsets.insert(i, cursor);
            let (size, _) = local_bytes(func.locals[i].ty);
            cursor += size as i32;
        }
        return AsyncSlots {
            offsets,
            frame_size: cursor,
        };
    };
    let packable: std::collections::HashSet<usize> = eligible
        .iter()
        .copied()
        .filter(|&i| packable(func.locals[i].ty))
        .collect();
    // Value-type locals alias their slot through a captured pointer, while
    // references/scalars hold copies re-read at poll entry — mixing the two
    // representations in one slot is unsound, so classes never cross kinds.
    let mut class_kind: Vec<bool> = Vec::new();

    // Greedy graph coloring in local-index order: each local takes the lowest
    // class none of whose members interfere with it. Deterministic (BTreeSet).
    let adj = super::passes::rc::liveness::frame_interference(func);
    let mut classes: Vec<Vec<usize>> = Vec::new();
    let mut class_of: HashMap<usize, usize> = HashMap::new();
    for &i in &eligible {
        if !packable.contains(&i) {
            continue;
        }
        let kind = interner.is_value_type(func.locals[i].ty);
        let mut placed = None;
        for (ci, members) in classes.iter().enumerate() {
            if class_kind[ci] == kind && members.iter().all(|&m| !adj[i].contains(&(m as u32))) {
                placed = Some(ci);
                break;
            }
        }
        let ci = match placed {
            Some(ci) => ci,
            None => {
                classes.push(Vec::new());
                class_kind.push(kind);
                classes.len() - 1
            }
        };
        classes[ci].push(i);
        class_of.insert(i, ci);
    }

    let mut cursor = slots_start;
    for members in &classes {
        let mut size = 8u32;
        for &i in members {
            let (sz, _) = local_bytes(func.locals[i].ty);
            size = size.max(sz);
        }
        cursor = (cursor + 7) & !7;
        for &i in members {
            offsets.insert(i, cursor);
        }
        cursor += size as i32;
    }
    // Locals excluded from packing each get their own slot, in index order.
    for &i in &eligible {
        if class_of.contains_key(&i) || offsets.contains_key(&i) {
            continue;
        }
        cursor = (cursor + 7) & !7;
        offsets.insert(i, cursor);
        let (size, _) = local_bytes(func.locals[i].ty);
        cursor += size as i32;
    }
    AsyncSlots {
        offsets,
        frame_size: cursor,
    }
}
