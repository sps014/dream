//! Async/await frame layout shared by the C backend.
//!
//! An `async fun` compiles to a **constructor** (allocates a `Future` frame, stores params,
//! enqueues the first poll, returns the frame pointer) and a **poll** function (resumable state
//! machine between `await` points). Saved locals are laid out in **local-index order** (not name
//! order), 8-byte aligned, starting at [`crate::abi::FutureLayout::slots`]. Field sizes differ per
//! target (wasm32 vs native pointer width), so callers supply them via the `local_bytes` closure.

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

/// Lays out Future-frame slots in **local-index order**, 8-byte aligned, starting at `slots_start`.
/// `local_bytes` returns `(size, is_value_type)` for a non-void local; void locals are omitted.
pub fn layout_async_slots(
    func: &MirFunction,
    interner: &TypeInterner,
    slots_start: i32,
    local_bytes: impl Fn(TypeId) -> (u32, bool),
) -> AsyncSlots {
    let mut offsets = HashMap::new();
    let mut cursor = slots_start;
    for (i, decl) in func.locals.iter().enumerate() {
        if matches!(interner.kind(decl.ty), TyKind::Void) {
            continue;
        }
        cursor = (cursor + 7) & !7;
        offsets.insert(i, cursor);
        let (size, _is_value) = local_bytes(decl.ty);
        cursor += size as i32;
    }
    AsyncSlots {
        offsets,
        frame_size: cursor,
    }
}
