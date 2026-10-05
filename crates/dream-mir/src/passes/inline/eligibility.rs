use super::remap::arg_type;
use super::remap::wasm_kind;
use super::remap::WasmKind;
use super::FnKey;
use crate::Operand;
use crate::Place;
use crate::Rvalue;
use crate::Statement;
use dream_types::TypeInterner;
use indexmap::IndexMap as HashMap;
use indexmap::IndexSet as HashSet;

/// A callee small enough to always inline: at most this many statements across all its blocks.
const MAX_INLINE_STMTS: usize = 64;
/// ...and at most this many blocks.
const MAX_INLINE_BLOCKS: usize = 16;
/// A resolved, inlinable call site within the caller.
pub(super) struct Site {
    /// Index of the caller block containing the call.
    pub(super) block: usize,
    /// Index of the call statement within that block.
    pub(super) stmt: usize,
    /// Index (in `mir.functions`) of the callee to inline.
    pub(super) callee: usize,
    /// The call's argument operands (caller-side, no remapping needed).
    pub(super) args: Vec<Operand>,
    /// Where the returned value is written (`None` for effect-only calls).
    pub(super) dest: Option<Place>,
}

/// Finds the first inlinable call site in function `fi`, or `None`.
#[allow(clippy::too_many_arguments)]
pub(super) fn find_site(
    mir: &crate::Mir,
    fi: usize,
    index: &HashMap<FnKey, usize>,
    recursive: &HashSet<FnKey>,
    call_counts: &HashMap<FnKey, usize>,
    addr_taken: &HashSet<FnKey>,
    interner: &TypeInterner,
) -> Option<Site> {
    let f = &mir.functions[fi];
    for (bi, block) in f.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            let (key, args, dest) = match stmt {
                Statement::Call { callee, args } => {
                    ((callee.def, callee.args.clone()), args.clone(), None)
                }
                Statement::Assign(place, Rvalue::Call { callee, args }) => (
                    (callee.def, callee.args.clone()),
                    args.clone(),
                    Some(place.clone()),
                ),
                _ => continue,
            };
            // Weak/unowned slots have their own ABI-result disposal rules; they cannot adopt
            // the owning return token like a local or strong container slot.
            if crate::passes::rc::field_store_is_non_strong(f, &mir.layouts, stmt) {
                continue;
            }
            let Some(&ci) = index.get(&key) else { continue };
            if !eligible(
                mir,
                fi,
                ci,
                &key,
                recursive,
                call_counts,
                addr_taken,
                &args,
                interner,
            ) {
                continue;
            }
            return Some(Site {
                block: bi,
                stmt: si,
                callee: ci,
                args,
                dest,
            });
        }
    }
    None
}

/// Whether callee `ci` may be inlined into caller `fi` at a site passing `n_args`.
#[allow(clippy::too_many_arguments)]
fn eligible(
    mir: &crate::Mir,
    fi: usize,
    ci: usize,
    key: &FnKey,
    recursive: &HashSet<FnKey>,
    call_counts: &HashMap<FnKey, usize>,
    addr_taken: &HashSet<FnKey>,
    args: &[Operand],
    interner: &TypeInterner,
) -> bool {
    if ci == fi {
        return false; // direct self-recursion
    }
    let g = &mir.functions[ci];
    let caller = &mir.functions[fi];
    if g.is_async {
        return false; // async bodies are stubs; real control flow lives in the HIR snapshot
    }
    if g.inline == dream_hir::InlineHint::Never {
        return false;
    }
    if recursive.contains(key) {
        return false; // part of a call cycle: inlining could not terminate
    }
    if g.name == crate::abi::ENTRY_FN || g.name == crate::lower::INIT_FN_NAME {
        return false;
    }
    // Keep `main` a thin driver. Single-use inlining of benches/`run_suite` into it produces one
    // multi-thousand-block function whose loops the optimizer no longer handles well.
    if caller.name == crate::abi::ENTRY_FN || caller.name == crate::lower::INIT_FN_NAME {
        return false;
    }
    if g.params.len() != args.len() || g.blocks.is_empty() {
        return false;
    }
    // Value-struct locals are OK: remapped borrows (`this`/`ref`/alias temps) stay aliases;
    // owning/param value locals get `manual_drop` + `ValueDrop` at each inlined return.
    // A call widens each argument to the callee's parameter WASM type at the boundary (e.g. `int` ->
    // `double`). Inlining replaces that with a binding, which must carry the same widening. We can only
    // emit the widening `Cast` when the argument's type is known. If a parameter's WASM type is wider
    // than `i32` and the argument's type is indeterminate (a field/index/global read), skip inlining
    // rather than risk an i32/i64/f32/f64 mismatch in the merged body.
    for (i, param) in g.params.iter().enumerate() {
        let pty = g.local_ty(*param);
        if wasm_kind(interner, pty) != WasmKind::I32
            && arg_type(caller, &args[i], interner).is_none()
        {
            return false;
        }
    }
    if addr_taken.contains(key) {
        return false;
    }
    let stmt_count: usize = g.blocks.iter().map(|b| b.stmts.len()).sum();
    let prefer = g.inline == dream_hir::InlineHint::Prefer;
    let (max_stmts, max_blocks) = if prefer {
        (128, 24)
    } else {
        (MAX_INLINE_STMTS, MAX_INLINE_BLOCKS)
    };
    let small = stmt_count <= max_stmts && g.blocks.len() <= max_blocks;
    if !small {
        return false;
    }
    // Keep small, multi-use wrappers thin (e.g. `Map.set` → `insert_no_grow`, `List.insert` →
    // `grow`). Inlining a large callee into the wrapper first would push it over the threshold and
    // prevent the wrapper from inlining into hot loops.
    let caller_key: FnKey = (caller.def, caller.instance.clone());
    let caller_sites = call_counts.get(&caller_key).copied().unwrap_or(0);
    if caller_sites > 1 {
        let caller_stmts: usize = caller.blocks.iter().map(|b| b.stmts.len()).sum();
        let caller_small =
            caller_stmts <= MAX_INLINE_STMTS && caller.blocks.len() <= MAX_INLINE_BLOCKS;
        if caller_small
            && !prefer
            && (caller_stmts + stmt_count > MAX_INLINE_STMTS
                || caller.blocks.len() + g.blocks.len() > MAX_INLINE_BLOCKS)
        {
            return false;
        }
    }
    true
}
