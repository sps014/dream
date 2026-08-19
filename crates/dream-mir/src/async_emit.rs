//! Async/await lowering for the MIR backend.
//!
//! An `async fun` compiles to a **constructor** (allocates a `Future` frame, stores params, enqueues
//! the first poll, returns the frame pointer) and a **poll** function (resumable state machine between
//! `await` points). The cooperative scheduler runtime lives in `mir/runtime/async.wat`.
//!
//! Saved locals are laid out in **local-index order** (not name order), 8-byte aligned, starting at
//! [`crate::abi::FutureLayout::slots`]. WASM uses wasm32 field sizes; the C backend uses native
//! pointer sizes from [`crate::abi::TargetAbi::native`].

use super::lower::lower_async_poll_body;
use super::MirFunction;
use crate::abi::{
    FutureLayout, FUTURE_KIND_ALL, FUTURE_KIND_ANY, FUTURE_KIND_HOST, FUTURE_KIND_TASK,
    FUTURE_STATUS_CANCELLED,
};
use crate::backend::shared::func_symbol;
use crate::backend::wasm::{emit_async_poll, poll_symbol, wasm_ty_of, FuncBuilder};
use dream_hir::scalar_size;
use dream_types::{TyKind, TypeId, TypeInterner};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

pub(crate) const F_STATE: i32 = FutureLayout::WASM32.state as i32;
pub(crate) const F_RESULT: i32 = FutureLayout::WASM32.result as i32;
pub(crate) const F_WIDE: i32 = FutureLayout::WASM32.wide as i32;
pub(crate) const F_AWAITING: i32 = FutureLayout::WASM32.awaiting as i32;
/// Byte size of a wasm32 `Future` frame's fixed header (locals start here). Host futures use this.
pub const F_SLOTS: i32 = FutureLayout::WASM32.slots as i32;
pub const KIND_HOST: i32 = FUTURE_KIND_HOST;
pub use crate::abi::HOST_POLL_INDEX;

const SLOT_SIZE: i32 = 8;

const RUNTIME_ASYNC: &str = include_str!("runtime/async.wat");

pub fn module_has_async(functions: &[MirFunction]) -> bool {
    functions.iter().any(|f| f.is_async)
}

pub fn async_runtime_wat() -> String {
    FutureLayout::WASM32
        .substitute_wat(RUNTIME_ASYNC)
        .replace("{KIND_ALL}", &FUTURE_KIND_ALL.to_string())
        .replace("{KIND_ANY}", &FUTURE_KIND_ANY.to_string())
        .replace("{KIND_HOST}", &FUTURE_KIND_HOST.to_string())
        .replace("{STATUS_CANCELLED}", &FUTURE_STATUS_CANCELLED.to_string())
        .replace("{tag_array}", &super::abi::TAG_ARRAY.to_string())
        .replace(
            "{RQ_HEAD_ADDR}",
            &super::abi::ASYNC_RQ_HEAD_ADDR.to_string(),
        )
        .replace(
            "{RQ_TAIL_ADDR}",
            &super::abi::ASYNC_RQ_TAIL_ADDR.to_string(),
        )
        .replace(
            "{TIMER_HEAD_ADDR}",
            &super::abi::ASYNC_TIMER_HEAD_ADDR.to_string(),
        )
        .replace("{VCLOCK_ADDR}", &super::abi::ASYNC_VCLOCK_ADDR.to_string())
}

pub(crate) struct AsyncSlots {
    /// `(local index, name, wasm type)` for every frame-resident local, in save/load order.
    pub(crate) entries: Vec<(usize, String, String)>,
    /// Local index → byte offset of its slot within the `Future` frame.
    pub(crate) offsets: HashMap<usize, i32>,
    /// Indices of reference-typed locals (retained across a suspend, released on completion).
    pub(crate) ref_locals: Vec<usize>,
    /// Value(`struct`)-typed locals get their own inline byte region in the frame (sized to the
    /// type's actual layout, not the uniform scalar `SLOT_SIZE`) rather than a saved/restored `i32`
    /// value: unlike a normal function's transient `$__sp` shadow stack (reused by unrelated calls
    /// between polls, so it cannot survive a suspend), the frame persists for the task's whole
    /// lifetime, so the local's WASM value is always just `self + offset` — recomputed fresh on
    /// every poll, never itself saved or restored (its bytes already live at that fixed address).
    pub(crate) value_locals: HashMap<usize, u32>,
    /// Byte size of the whole frame (header + slots).
    pub(crate) frame_size: i32,
}

/// Lays out Future-frame slots in **local-index order**, 8-byte aligned, starting at `slots_start`.
/// `local_bytes` returns `(size, is_value_type)` for a non-void local; void locals are omitted.
pub(crate) fn layout_async_slots(
    func: &MirFunction,
    interner: &TypeInterner,
    slots_start: i32,
    ty_name: impl Fn(TypeId) -> String,
    local_bytes: impl Fn(TypeId) -> (u32, bool),
) -> AsyncSlots {
    let mut entries = Vec::new();
    let mut offsets = HashMap::new();
    let mut ref_locals = Vec::new();
    let mut value_locals = HashMap::new();
    let mut cursor = slots_start;
    for (i, decl) in func.locals.iter().enumerate() {
        if matches!(interner.kind(decl.ty), TyKind::Void) {
            continue;
        }
        cursor = (cursor + 7) & !7;
        offsets.insert(i, cursor);
        let name = decl.name.clone().unwrap_or_else(|| format!("_{i}"));
        entries.push((i, name, ty_name(decl.ty)));
        let (size, is_value) = local_bytes(decl.ty);
        if is_value {
            value_locals.insert(i, size);
        } else if interner.is_rc_tracked(decl.ty) {
            ref_locals.push(i);
        }
        cursor += size as i32;
    }
    AsyncSlots {
        entries,
        offsets,
        ref_locals,
        value_locals,
        frame_size: cursor,
    }
}

fn async_slots(func: &MirFunction, interner: &TypeInterner) -> AsyncSlots {
    layout_async_slots(
        func,
        interner,
        F_SLOTS,
        |ty| wasm_ty_of(interner, ty).to_string(),
        |ty| {
            if interner.is_value_type(ty) {
                (scalar_size(interner, ty).0.max(SLOT_SIZE as u32), true)
            } else {
                (SLOT_SIZE as u32, false)
            }
        },
    )
}

pub(crate) fn slot_store(wt: &str) -> &'static str {
    match wt {
        "f64" => "f64.store",
        "f32" => "f32.store",
        "i64" => "i64.store",
        _ => "i32.store",
    }
}

/// Emits the constructor WAT and poll [`FuncBuilder`] for one async function.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_async_function_parts(
    func: &MirFunction,
    interner: &TypeInterner,
    symbols: &HashMap<(dream_types::DefId, Vec<TypeId>), String>,
    layouts: &dream_hir::LayoutTable,
    strings: &IndexMap<String, u32>,
    tags: &HashMap<TypeId, i32>,
    ftable: &HashMap<(dream_types::DefId, Vec<TypeId>), usize>,
    defined_funcs: &HashSet<(dream_types::DefId, Vec<TypeId>)>,
    value_glue: &HashSet<TypeId>,
    poll_idx: usize,
    debug: bool,
    locate_panics: bool,
    debug_fn: Option<&crate::backend::wasm::debug_map::DebugFunction>,
    intrinsics: &HashMap<dream_types::DefId, dream_abi::intrinsics::IntrinsicOp>,
) -> (String, FuncBuilder) {
    let hir = func.hir_fn.as_ref().unwrap_or_else(|| {
        crate::internal_error!(
            "async function '{}' reached codegen without its HIR snapshot",
            func.name
        )
    });
    // The coroutine body carries all frame-resident locals (user locals + await/scratch temps).
    // Poll MIR is lowered here (stubs skip module-wide RcInsertion), so insert RC on this CFG
    // before emit — otherwise mid-body aliasing/reassign and return handoff corrupt counts.
    let mut body = lower_async_poll_body(hir, interner);
    let param_is_sink: Vec<bool> = hir.params.iter().map(|p| p.is_take).collect();
    // Frame owns each RC param's +1 (sink transfer or borrow retain in the ctor). Mark them take
    // so poll RcInsertion releases them at AsyncComplete.
    for p in &body.params {
        let decl = &mut body.locals[p.0 as usize];
        if interner.is_rc_tracked(decl.ty) {
            decl.is_take = true;
        }
    }
    let _ = crate::passes::RcInsertion::run_with_layouts(&mut body, interner, layouts);
    let slots = async_slots(&body, interner);
    let frame_size = slots.frame_size;
    let sym = func_symbol(func);
    let mut out = String::new();

    // Constructor: allocate the future frame, store params into slots, enqueue the first poll, and
    // hand the frame back. Sink RC params already hold the caller's +1 — do not retain again.
    // Borrow RC params are retained so the frame owns a copy independent of the caller.
    if debug {
        let _ = writeln!(out, "(func ${sym} (@name \"{}\")", func.name);
    } else {
        let _ = writeln!(out, "(func ${sym}");
    }
    for p in &body.params {
        let name = &body.locals[p.0 as usize].name;
        if debug && name.is_some() {
            let _ = writeln!(
                out,
                " (param ${} (@name \"{}\") {})",
                p.0,
                name.as_ref().unwrap(),
                wasm_ty_of(interner, body.locals[p.0 as usize].ty)
            );
        } else {
            let _ = writeln!(
                out,
                " (param ${} {})",
                p.0,
                wasm_ty_of(interner, body.locals[p.0 as usize].ty)
            );
        }
    }
    out.push_str(" (result i32)\n (local $self i32)\n");
    let _ = writeln!(out, " i32.const {frame_size}");
    let _ = writeln!(out, " i32.const {poll_idx}");
    let _ = writeln!(out, " i32.const {FUTURE_KIND_TASK}");
    out.push_str(" call $dream_new_future\n local.set $self\n");
    for (pi, p) in body.params.iter().enumerate() {
        let idx = p.0 as usize;
        let off = slots.offsets[&idx];
        if let Some(&size) = slots.value_locals.get(&idx) {
            // Same by-value contract as `emit_value_frame_prologue`: memcpy only. Still-live
            // callers emit `ValueRetain` before the async ctor; last-use callers `ValueKill`
            // after so this frame inherits the nested refs.
            let _ = writeln!(
                out,
                " local.get $self\n i32.const {off}\n i32.add\n local.get ${idx}\n i32.const {size}\n memory.copy"
            );
            continue;
        }
        let wt = wasm_ty_of(interner, body.locals[idx].ty);
        if interner.is_rc_tracked(body.locals[idx].ty) {
            let sink = param_is_sink.get(pi).copied().unwrap_or(true);
            // Sink-default: caller already transferred +1. Borrow: retain for the frame.
            if !sink {
                let _ = writeln!(out, " local.get ${idx}");
                let retain = match interner.kind(body.locals[idx].ty) {
                    dream_types::TyKind::Js => "$js_retain",
                    _ if interner.is_shared_type(body.locals[idx].ty) => "$retain_shared",
                    _ => "$retain",
                };
                let _ = writeln!(out, " call {retain}");
            }
        }
        let _ = writeln!(
            out,
            " local.get $self\n local.get ${idx}\n {} offset={off}",
            slot_store(wt)
        );
    }
    out.push_str(" local.get $self\n call $dream_enqueue\n local.get $self\n)\n\n");

    let user_local_count = hir.params.len() + hir.locals.len();
    let poll = emit_async_poll(
        &body,
        interner,
        symbols,
        layouts,
        strings,
        tags,
        ftable,
        defined_funcs,
        value_glue,
        &slots,
        &poll_symbol(func),
        user_local_count,
        debug,
        locate_panics,
        debug_fn,
        intrinsics,
    );
    (out, poll)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn async_runtime_has_no_placeholders() {
        let wat = async_runtime_wat();
        assert!(!wat.contains('{') && !wat.contains('}'));
    }
}
