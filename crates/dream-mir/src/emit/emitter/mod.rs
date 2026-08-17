//! Emits one MIR function as WAT text. Core function-shell emission (locals + shaped sync body)
//! plus the low-level operand/type helpers shared by every other emitter submodule live here; the
//! larger emission concerns are split out:
//! - [`statements`]: `Statement` emission (assignment, print, retain/release, calls) and the
//!   place-store helpers (retain-on-store, deferred release of an overwritten reference).
//! - [`value_struct`]: the value(`struct`)/value-union inline (non-heap) representation — shadow
//!   frame prologue/teardown, in-place construction, byte-copy, and sret-call helpers.
//! - [`terminator`]: `Terminator` emission (branches, returns, tail calls) and the dynamic `js` call
//!   marshaling helper. Sync CFG edges use [`shape`]; async poll still uses `$__pc` via this module.
//! - [`shape`]: relooper-shaped nested `block`/`loop`/`if` emission for sync functions.
//! - [`async_ops`]: async-coroutine poll emission (split out previously).
//! - [`rvalue`]: `Rvalue` (expression) emission (split out previously).

use super::*;
use crate::async_emit::{slot_load, slot_store, AsyncSlots, F_AWAITING, F_RESULT, F_STATE, F_WIDE};
use crate::emit::valuetype::{is_simd_vector, ValueFrame, ValueLocalKind};
use std::collections::HashSet;

mod async_ops;
mod rvalue;
mod shape;
mod simd;
mod statements;
mod terminator;
mod value_struct;

/// Emits one function as WAT (calls fall back to `$def{N}`, and field/index access has no layout, so
/// this is for layout-free unit tests; the pipeline uses [`emit_program`]/[`emit_module`]).
pub fn emit_function(func: &MirFunction, interner: &TypeInterner) -> String {
    let empty_globals = HashMap::new();
    emit_function_with(
        func,
        interner,
        &HashMap::new(),
        &HashMap::new(),
        &LayoutTable::default(),
        &IndexMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashSet::new(),
        &empty_globals,
        &HashSet::new(),
        false,
        true,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_function_with(
    func: &MirFunction,
    interner: &TypeInterner,
    symbols: &HashMap<(DefId, Vec<TypeId>), String>,
    sigs: &HashMap<(DefId, Vec<TypeId>), Vec<TypeId>>,
    layouts: &LayoutTable,
    strings: &IndexMap<String, u32>,
    tags: &HashMap<TypeId, i32>,
    func_table: &HashMap<(DefId, Vec<TypeId>), usize>,
    value_glue: &HashSet<TypeId>,
    global_tys: &HashMap<u32, TypeId>,
    defined_funcs: &HashSet<(DefId, Vec<TypeId>)>,
    debug: bool,
    locate_panics: bool,
    debug_fn: Option<&crate::emit::debug_map::DebugFunction>,
) -> String {
    let mut e = Emitter::new(
        func,
        interner,
        symbols,
        sigs,
        layouts,
        strings,
        tags,
        func_table,
        defined_funcs,
        value_glue,
        global_tys,
        None,
        0,
        debug,
        locate_panics,
        debug_fn,
    );
    e.emit();
    e.out
}

/// Emits the poll function of an async coroutine: a single state-machine dispatch over the full
/// lowered body (`func`), whose `Await` terminators suspend/resume. `slots` maps every frame-resident
/// local to its offset in the `Future` frame. See [`Emitter::emit_async_state_machine`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_async_poll(
    func: &MirFunction,
    interner: &TypeInterner,
    symbols: &HashMap<(DefId, Vec<TypeId>), String>,
    layouts: &LayoutTable,
    strings: &IndexMap<String, u32>,
    tags: &HashMap<TypeId, i32>,
    ftable: &HashMap<(DefId, Vec<TypeId>), usize>,
    defined_funcs: &HashSet<(DefId, Vec<TypeId>)>,
    value_glue: &HashSet<TypeId>,
    slots: &AsyncSlots,
    poll_sym: &str,
    user_local_count: usize,
    debug: bool,
    locate_panics: bool,
    debug_fn: Option<&crate::emit::debug_map::DebugFunction>,
) -> String {
    // Async bodies do not apply call-argument widening or value-struct shadow frames (frame storage
    // lives in the Future); empty maps disable those paths without extra plumbing.
    let sigs: HashMap<(DefId, Vec<TypeId>), Vec<TypeId>> = HashMap::new();
    let global_tys: HashMap<u32, TypeId> = HashMap::new();
    // The poll body *is* the coroutine; completions release its own reference locals.
    let mut e = Emitter::new(
        func,
        interner,
        symbols,
        &sigs,
        layouts,
        strings,
        tags,
        ftable,
        defined_funcs,
        value_glue,
        &global_tys,
        Some(func),
        user_local_count,
        debug,
        locate_panics,
        debug_fn,
    );
    e.emit_async_state_machine(slots, poll_sym);
    e.out
}

struct Emitter<'a> {
    func: &'a MirFunction,
    interner: &'a TypeInterner,
    symbols: &'a HashMap<(DefId, Vec<TypeId>), String>,
    /// Callee `(def, instance)` → parameter types, for implicit widening of call arguments.
    sigs: &'a HashMap<(DefId, Vec<TypeId>), Vec<TypeId>>,
    layouts: &'a LayoutTable,
    strings: &'a IndexMap<String, u32>,
    tags: &'a HashMap<TypeId, i32>,
    func_table: &'a HashMap<(DefId, Vec<TypeId>), usize>,
    /// In-module function keys (not host imports); used to keep boxed `fun` values boxed on internal calls.
    defined_funcs: &'a HashSet<(DefId, Vec<TypeId>)>,
    /// Value-struct types that require retain/drop glue (see [`valuetype`]).
    value_glue: &'a HashSet<TypeId>,
    /// Module global id → type (for value-struct global stores/addresses).
    global_tys: &'a HashMap<u32, TypeId>,
    /// Shadow-frame layout + ownership classification of this function's value-struct locals.
    frame: ValueFrame,
    out: String,
    /// When emitting inside an async poll segment, the enclosing task (for scope-exit release).
    async_parent: Option<&'a MirFunction>,
    /// In an async poll body, the count of persistent user locals (params + declared `let`s) at the
    /// front of `func.locals`; only these get value(`struct`) drop glue on completion. RC locals are
    /// released by MIR `Release` stmts from poll `RcInsertion`. Synthetic temps that follow are
    /// transient (their RC is still MIR-managed when owned).
    async_user_locals: usize,
    /// Generate `@name` annotations
    debug: bool,
    /// When true, [`Self::emit_panic`] builds file:line messages matching the string table; when
    /// false (release), it uses the shared base message only.
    locate_panics: bool,
    /// Debug-info metadata for this function when compiled with source-level debug-info (line hooks
    /// + local spilling). `None` disables all instrumentation (release builds, async bodies).
    debug_fn: Option<&'a crate::emit::debug_map::DebugFunction>,
    /// The most recent [`Statement::SourceLine`] seen while emitting this function's statements (0
    /// before the first one). Read by [`Self::emit_panic`] to attribute an automatic runtime check to
    /// a real source line; see [`crate::emit::panic_msgs`]. `mir::emit::strings::string_table`
    /// tracks this identically while scanning the same (already fully-optimized) MIR, so the two
    /// stay in lockstep and every message `emit_panic` looks up is guaranteed pre-interned.
    current_line: u32,
    /// Nested continue/break labels for [`shape`] emission (sync only; empty for async poll).
    shape_scopes: Vec<shape::ShapeScope>,
    /// Monotonic id for `$__brkN` / `$__cntN` / `$__joinN` labels in shape emit.
    shape_label_id: u32,
}

impl<'a> Emitter<'a> {
    /// Builds an emitter for `func`, computing its value-struct shadow frame. `async_parent`/
    /// `async_user_locals` are set only for async poll bodies (see [`emit_async_poll`]); the ordinary
    /// function path passes `None`/`0`. Shared by [`emit_function_with`] and [`emit_async_poll`].
    #[allow(clippy::too_many_arguments)]
    fn new(
        func: &'a MirFunction,
        interner: &'a TypeInterner,
        symbols: &'a HashMap<(DefId, Vec<TypeId>), String>,
        sigs: &'a HashMap<(DefId, Vec<TypeId>), Vec<TypeId>>,
        layouts: &'a LayoutTable,
        strings: &'a IndexMap<String, u32>,
        tags: &'a HashMap<TypeId, i32>,
        func_table: &'a HashMap<(DefId, Vec<TypeId>), usize>,
        defined_funcs: &'a HashSet<(DefId, Vec<TypeId>)>,
        value_glue: &'a HashSet<TypeId>,
        global_tys: &'a HashMap<u32, TypeId>,
        async_parent: Option<&'a MirFunction>,
        async_user_locals: usize,
        debug: bool,
        locate_panics: bool,
        debug_fn: Option<&'a crate::emit::debug_map::DebugFunction>,
    ) -> Self {
        let frame = ValueFrame::compute(func, interner, layouts);
        Emitter {
            func,
            interner,
            symbols,
            sigs,
            layouts,
            strings,
            tags,
            func_table,
            defined_funcs,
            value_glue,
            global_tys,
            frame,
            out: String::new(),
            async_parent,
            async_user_locals,
            debug,
            locate_panics,
            debug_fn,
            current_line: 0,
            shape_scopes: Vec::new(),
            shape_label_id: 0,
        }
    }
}

impl Emitter<'_> {
    fn line(&mut self, s: &str) {
        let _ = writeln!(self.out, "{}", s);
    }

    /// The symbol for a call target: the resolved function symbol for `(def, instance args)` when
    /// known, else a `def{N}` fallback (runtime intrinsics and not-yet-emitted targets).
    fn callee_symbol(&self, callee: &crate::Callee) -> String {
        self.symbols
            .get(&(callee.def, callee.args.clone()))
            .cloned()
            .or_else(|| self.symbols.get(&(callee.def, vec![])).cloned())
            .unwrap_or_else(|| format!("def{}", callee.def.0))
    }

    fn emit(&mut self) {
        let mut params: String = self
            .func
            .params
            .iter()
            .map(|p| {
                let p_ty = self.wasm_ty(self.func.local_ty(*p));
                let name = &self.func.locals[p.0 as usize].name;
                if self.debug && name.is_some() {
                    format!(
                        " (param ${} (@name \"{}\") {})",
                        p.0,
                        name.as_ref().unwrap(),
                        p_ty
                    )
                } else {
                    format!(" (param ${} {})", p.0, p_ty)
                }
            })
            .collect();
        // A value(`struct`)-returning function uses the sret ABI: a hidden leading `$__sret` pointer
        // names the caller-provided destination the result is copied into, and the function itself
        // returns no WASM value.
        let result = if self.returns_value_struct() {
            params = format!(" (param $__sret i32){}", params);
            String::new()
        } else {
            match self.interner.kind(self.func.ret) {
                TyKind::Void => String::new(),
                _ => format!(" (result {})", self.wasm_ty(self.func.ret)),
            }
        };
        let sym = func_symbol(self.func);
        if self.debug {
            self.line(&format!(
                "(func ${} (@name \"{}\"){}{}",
                sym, self.func.name, params, result
            ));
        } else {
            self.line(&format!("(func ${}{}{}", sym, params, result));
        }

        // Non-parameter locals. `$__pc` is only needed when shape emit falls back to PC dispatch
        // (multi-entry loop bodies); declare it whenever the relooper shape needs that path.
        let param_count = self.func.params.len();
        for (i, decl) in self.func.locals.iter().enumerate() {
            if i < param_count {
                continue;
            }
            let wasm = self.wasm_local_ty(decl.ty);
            if let (true, Some(name)) = (self.debug, decl.name.as_ref()) {
                self.line(&format!("  (local ${} (@name \"{}\") {})", i, name, wasm));
            } else {
                self.line(&format!("  (local ${} {})", i, wasm));
            }
        }
        // Always reserve `$__pc`: shape emit may fall back to PC dispatch mid-body decision, and the
        // local is cheap. Async poll declares its own copy in `emit_async_state_machine`.
        self.line("  (local $__pc i32)");
        if self.frame.size > 0 {
            // Saved shadow-stack pointer, restored before every return.
            self.line("  (local $__saved_sp i32)");
        }
        // Scratch pointer holding the object under construction across field initialization
        // (`New`/`ArrayLit`). Safe as a single slot: lowering materializes all args into operands,
        // so allocations never nest within a single rvalue.
        self.line("  (local $__obj i32)");
        // Scratch length for `Buffer.alloc<T>(len)`: the count is needed for both the allocation size
        // and the zero-fill, so it is materialized once here.
        self.line("  (local $__len i32)");
        // Scratch holding the old element count across a `Buffer.realloc<T>` (needed both to size the
        // `$realloc` call and to zero-fill only the newly grown tail, if any).
        self.line("  (local $__old_len i32)");
        // Scratch holding the previous occupant of a reference field/element across a reassignment, so
        // it can be released *after* the new value is stored (deferred release keeps a self-referential
        // `obj.f = g(obj.f)` sound).
        self.line("  (local $__rel i32)");
        // Scratch holding the saved `$__sp` across a dynamic `js` call's argument-slot buffer (see
        // `emit_js_call`): the buffer is bump-allocated below `$__sp` and released right after the
        // single host crossing, so this need only survive one rvalue.
        self.line("  (local $__jsp i32)");
        // Scratch pointers used only by `weak`/`unowned` field stores (see `emit_weak_field_store`/
        // `emit_unowned_field_store` in `statements.rs`): `$__wsrc` holds the freshly evaluated RHS
        // `Option<T>` value (for `weak`) or plain reference value (for `unowned`) while the old
        // occupant is torn down; `$__wbox` holds the freshly allocated private weak-box pointer.
        self.line("  (local $__wsrc i32)");
        self.line("  (local $__wbox i32)");
        // Scratch holding the source array/buffer pointer across a `T[]` `ToBytes`/`FromBytes`
        // dynamic-length raw copy (see `Rvalue::ToBytes`/`FromBytes` in `rvalue/mod.rs`), needed
        // once the destination allocation starts overwriting `$__obj`.
        self.line("  (local $__src i32)");
        self.line("  (local $__v128 v128)");

        self.emit_value_frame_prologue();
        // Debug-info: announce entry into this function so the debugger can push a call-stack frame.
        if let Some(dbg) = self.debug_fn {
            self.line(&format!("  (call $__dbg_enter (i32.const {}))", dbg.id));
        }
        self.emit_shaped_body();
        self.line(")");
    }

    /// Emits the `dream_debug.exit` hook (pops the debugger's call-stack frame) right before a return,
    /// when debug-info is on. Placed at every real return site so the shadow call stack stays
    /// balanced regardless of which path exits the function.
    fn emit_debug_exit(&mut self) {
        if let Some(dbg) = self.debug_fn {
            self.line(&format!("     (call $__dbg_exit (i32.const {}))", dbg.id));
        }
    }

    /// The runtime tag to stamp into a newly allocated value of `ty`: its assigned struct/union tag,
    /// or the `DefId` as a last-resort fallback (only when no layout/tag is registered).
    fn type_tag(&self, ty: TypeId, fallback: DefId) -> i32 {
        self.tags.get(&ty).copied().unwrap_or(fallback.0 as i32)
    }

    /// The heap address of an interned string. Every string literal reachable in codegen is
    /// harvested into the interner beforehand (see `strings_in_*`), so a miss is a harvesting bug,
    /// not a user error — fail loudly instead of emitting a null (address 0) string.
    fn string_addr(&self, s: &str) -> u32 {
        self.strings.get(s).copied().unwrap_or_else(|| {
            crate::internal_error!("string literal {:?} was not interned before codegen", s)
        })
    }

    /// Emits `(i32.const <addr>) (call $dream_panic)` for one of the fixed panic message bases (see
    /// [`super::panic_msgs`]) — the shared halt point every automatic runtime check (bounds,
    /// division by zero, bad cast) funnels through. The message is *located*: `base` is combined
    /// with the current function's file/name (already on [`Emitter::func`]) and [`Self::current_line`]
    /// (kept up to date by `Statement::SourceLine` markers as statements are emitted — see
    /// [`super::panic_msgs::located`]) so the failure is attributable to a real source line without
    /// threading source spans through MIR. Always emitted inside an unconditional branch of an `if`;
    /// the caller is responsible for the guarding comparison.
    fn emit_panic(&mut self, base: &str) {
        let msg = if self.locate_panics {
            super::panic_msgs::located(
                base,
                self.func.file.as_deref(),
                &self.func.name,
                self.current_line,
            )
        } else {
            base.to_string()
        };
        self.line(&format!("     (i32.const {})", self.string_addr(&msg)));
        self.line("     (call $dream_panic)");
    }

    /// Traps if the just-loaded `unowned` field value (top of stack) is the poison sentinel `0`
    /// (written by `$weak_clear_all` when the referent was freed — see `src/mir/runtime/weak.wat`),
    /// otherwise leaves the value back on the stack unchanged. Runtime semantics for `unowned`: see
    /// `docs/language/memory.md`.
    fn emit_unowned_read_check(&mut self) {
        self.line("     (local.set $__wsrc)");
        self.line("     (local.get $__wsrc)");
        self.line("     (i32.eqz)");
        self.line("     (if (then");
        self.emit_panic(super::panic_msgs::UNOWNED_NULL_DEREF);
        self.line("     ))");
        self.line("     (local.get $__wsrc)");
    }

    /// Pushes the address of `base.field` (`base + offset`) onto the stack.
    fn field_addr(&mut self, base: crate::Local, offset: u32) {
        self.line(&format!("     (local.get ${})", base.0));
        if offset > 0 {
            self.line(&format!("     (i32.const {})", offset));
            self.line("     (i32.add)");
        }
    }

    /// Traps via `$dream_panic` if `index` (reinterpreted as unsigned, so a negative index also
    /// trips it as a huge positive value) is `>=` the length stored at `base`'s heap header (offset
    /// 0 — see the array/string layout comments on [`elem_addr`]/`$char_at`). `len_addr` computes
    /// the address holding the length (usually just `base`, but callers pass a projected address
    /// for other layouts, e.g. a string's payload pointer).
    fn emit_bounds_check(&mut self, len_addr: impl FnOnce(&mut Self), index: &Operand) {
        self.emit_operand(index);
        len_addr(self);
        self.line("     (i32.load)");
        self.line("     (i32.ge_u)");
        self.line("     (if (then");
        self.emit_panic(super::panic_msgs::INDEX_OUT_OF_BOUNDS);
        self.line("     ))");
    }

    /// Pushes the address of `base[index]` (`base + 4 + index * elem_size`) onto the stack. The
    /// length occupies the first word, so element 0 is at offset 4. Checked: traps via `$dream_panic`
    /// if `index` is out of range (see [`Self::emit_bounds_check`]).
    fn elem_addr(&mut self, base: crate::Local, elem_ty: TypeId, index: &Operand, unchecked: bool) {
        let (size, _) = scalar_size(self.interner, elem_ty);
        if !unchecked {
            self.emit_bounds_check(|s| s.line(&format!("     (local.get ${})", base.0)), index);
        }
        self.line(&format!("     (local.get ${})", base.0));
        self.line("     (i32.const 4)");
        self.line("     (i32.add)");
        self.emit_operand(index);
        self.line(&format!("     (i32.const {})", size));
        self.line("     (i32.mul)");
        self.line("     (i32.add)");
    }

    /// Emits `s[i]` as an inlined UTF-16 unit load. When `unchecked` is false, a located
    /// `index >= unit_len` check runs first (precise file:line panic).
    fn emit_char_at(&mut self, s: &Operand, i: &Operand, unchecked: bool) {
        if !unchecked {
            self.emit_operand(i);
            self.emit_operand(s);
            self.line("     (i32.load) ;; unit_len");
            self.line("     (i32.ge_u)");
            self.line("     (if (then");
            self.emit_panic(super::panic_msgs::INDEX_OUT_OF_BOUNDS);
            self.line("     ))");
        }
        self.emit_operand(s);
        self.line(&format!(
            "     (i32.const {})",
            crate::abi::STRING_UTF8_OFFSET
        ));
        self.line("     (i32.add)");
        self.emit_operand(i);
        self.line("     (i32.const 1)");
        self.line("     (i32.shl)");
        self.line("     (i32.add)");
        self.line("     (i32.load16_u)");
    }

    /// Emits `s.byte_at(i)` as an inlined payload byte load, with an optional byte-length check.
    fn emit_byte_at(&mut self, s: &Operand, i: &Operand, unchecked: bool) {
        if !unchecked {
            self.emit_operand(i);
            self.emit_operand(s);
            self.line("     (i32.load)");
            self.line("     (i32.const 1)");
            self.line("     (i32.shl) ;; byte_size");
            self.line("     (i32.ge_u)");
            self.line("     (if (then");
            self.emit_panic(super::panic_msgs::INDEX_OUT_OF_BOUNDS);
            self.line("     ))");
        }
        self.emit_operand(s);
        self.line(&format!(
            "     (i32.const {})",
            crate::abi::STRING_UTF8_OFFSET
        ));
        self.line("     (i32.add)");
        self.emit_operand(i);
        self.line("     (i32.add)");
        self.line("     (i32.load8_u)");
    }

    /// The struct field's `(byte offset, type)` from the layout table, or `None` when `base` is not a
    /// laid-out nominal type (e.g. a union, or a type whose layout was not recorded).
    fn field_layout(&self, base: crate::Local, field: usize) -> Option<(u32, TypeId)> {
        let bty = self.func.local_ty(base);
        // Layouts are keyed by the full (monomorphized) type id, so `Box<int>` and `Box<string>`
        // resolve to their own field widths.
        let f = self.layouts.get(bty)?.fields.get(field)?;
        Some((f.offset, f.ty))
    }

    /// Like [`Self::field_layout`], but also exposes the field's `weak`/`unowned` storage qualifiers
    /// (see `docs/language/memory.md`), needed at both the store site (skip retain, register in the
    /// weak side-table) and the read site (trap on a poisoned `unowned` field).
    fn field_layout_full(
        &self,
        base: crate::Local,
        field: usize,
    ) -> Option<&dream_hir::FieldLayout> {
        let bty = self.func.local_ty(base);
        self.layouts.get(bty)?.fields.get(field)
    }

    /// The element type of an array-typed local, or `None` if `base` is not an array.
    fn array_elem_ty(&self, base: crate::Local) -> Option<TypeId> {
        match self.interner.kind(self.func.local_ty(base)) {
            TyKind::Array(e) => Some(*e),
            _ => None,
        }
    }

    /// The load instruction for a value of `ty` (width- and float-aware; sub-word loads are unsigned).
    fn load_instr(&self, ty: TypeId) -> &'static str {
        load_instr_for(self.interner, ty)
    }

    /// The store instruction matching [`Self::load_instr`].
    fn store_instr(&self, ty: TypeId) -> &'static str {
        store_instr_for(self.interner, ty)
    }

    fn emit_operand(&mut self, op: &Operand) {
        match op {
            Operand::Const(c) => self.emit_const(c),
            Operand::Copy(Place::Local(l)) => self.line(&format!("     (local.get ${})", l.0)),
            Operand::Copy(Place::Global(g)) => self.line(&format!("     (global.get $g{})", g.0)),
            Operand::Copy(Place::Field { base, field }) => {
                if self.is_v128_local(*base) {
                    let lane = self
                        .layouts
                        .get(self.func.local_ty(*base))
                        .and_then(|l| l.fields.get(*field))
                        .map(|f| f.offset / 4)
                        .unwrap_or(0);
                    self.line(&format!("     (local.get ${})", base.0));
                    self.line(&format!("     (i32x4.extract_lane {lane})"));
                    return;
                }
                if let Some(f) = self.field_layout_full(*base, *field) {
                    let (off, fty, is_unowned) = (f.offset, f.ty, f.is_unowned);
                    self.field_addr(*base, off);
                    // A value-struct field is addressed inline, not loaded: reading it yields the
                    // address of its inline storage (the consumer copies where a value is needed).
                    if !self.interner.is_value_type(fty) {
                        self.line(&format!("     ({})", self.load_instr(fty)));
                    }
                    if is_unowned {
                        self.emit_unowned_read_check();
                    }
                } else {
                    crate::internal_error!(
                        "missing field layout for read (base {:?}, field {})",
                        base,
                        field
                    );
                }
            }
            Operand::Copy(Place::Index {
                base,
                index,
                unchecked,
            }) => {
                if let Some(ety) = self.array_elem_ty(*base) {
                    self.elem_addr(*base, ety, index, *unchecked);
                    if !self.interner.is_value_type(ety) {
                        self.line(&format!("     ({})", self.load_instr(ety)));
                    }
                } else {
                    crate::internal_error!("missing array element type for read (base {:?})", base);
                }
            }
            Operand::Copy(Place::Deref { ptr, elem_ty }) => {
                self.line(&format!("     (local.get ${})", ptr.0));
                if !self.interner.is_value_type(*elem_ty) {
                    self.line(&format!("     ({})", self.load_instr(*elem_ty)));
                }
            }
        }
    }

    fn emit_const(&mut self, c: &Const) {
        match c {
            Const::Int(v) => self.line(&format!("     (i32.const {})", v)),
            Const::Long(v) => self.line(&format!("     (i64.const {})", v)),
            Const::Float(v) => self.line(&format!("     (f64.const {})", v)),
            Const::F32(v) => self.line(&format!("     (f32.const {})", v)),
            Const::Bool(v) => self.line(&format!("     (i32.const {})", *v as i32)),
            Const::Char(v) => self.line(&format!("     (i32.const {})", *v as u32)),
            Const::Null => self.line("     (i32.const 0)"),
            Const::Str(s) => match self.strings.get(s) {
                Some(addr) => self.line(&format!("     (i32.const {})", addr)),
                None => crate::internal_error!("missing interned string: {}", s),
            },
        }
    }

    pub(super) fn operand_ty(&self, op: &Operand) -> TypeId {
        match op {
            Operand::Copy(Place::Local(l)) => self.func.local_ty(*l),
            Operand::Copy(Place::Field { base, field }) => self
                .field_layout(*base, *field)
                .map(|(_, t)| t)
                .unwrap_or_else(|| self.func.local_ty(*base)),
            Operand::Copy(Place::Index { base, .. }) => self
                .array_elem_ty(*base)
                .unwrap_or_else(|| self.func.local_ty(*base)),
            Operand::Copy(Place::Deref { elem_ty, .. }) => *elem_ty,
            Operand::Copy(Place::Global(_)) => self.interner.int(),
            Operand::Const(Const::Long(_)) => self.interner.long(),
            Operand::Const(Const::Float(_)) => self.interner.double(),
            Operand::Const(Const::F32(_)) => self.interner.float(),
            // A char/bool/string constant keeps its own primitive type so type-directed dispatch
            // (e.g. `to_string`/`hash_code`, boxing into `object`) picks the right helper rather than
            // defaulting to `int`.
            Operand::Const(Const::Char(_)) => self.interner.char(),
            Operand::Const(Const::Bool(_)) => self.interner.bool(),
            Operand::Const(Const::Str(_)) => self.interner.string(),
            Operand::Const(_) => self.interner.int(),
        }
    }

    fn wasm_ty(&self, ty: TypeId) -> String {
        wasm_ty_of(self.interner, ty).to_string()
    }

    fn wasm_local_ty(&self, ty: TypeId) -> &'static str {
        if is_simd_vector(self.layouts, ty) {
            "v128"
        } else {
            wasm_ty_of(self.interner, ty)
        }
    }

    fn is_v128_local(&self, l: crate::Local) -> bool {
        let i = l.0 as usize;
        if i < self.func.params.len() {
            return false;
        }
        let decl = &self.func.locals[i];
        is_simd_vector(self.layouts, decl.ty)
    }

    fn emit_v128_spill_addr(&mut self) {
        let off = self.frame.v128_spill.unwrap_or(0);
        self.line("     (global.get $__sp)");
        if off > 0 {
            self.line(&format!("     (i32.const {off})"));
            self.line("     (i32.add)");
        }
    }

    fn emit_v128_sret_into_local(&mut self, dest: u32, callee: &crate::Callee, args: &[Operand]) {
        self.emit_v128_spill_addr();
        self.emit_call_args(callee, args);
        self.line(&format!("     (call ${})", self.callee_symbol(callee)));
        self.emit_v128_spill_addr();
        self.line("     (v128.load)");
        self.line(&format!("     (local.set ${dest})"));
    }

    fn binop_instr(&self, op: BinOp, ty: TypeId) -> String {
        let w = self.wasm_ty(ty);
        let signed = !matches!(
            self.interner.kind(ty),
            TyKind::Prim(PrimTy::UInt | PrimTy::ULong | PrimTy::Byte)
        );
        let s = if signed { "_s" } else { "_u" };
        let is_float = w == "f32" || w == "f64";
        match op {
            BinOp::Add => format!("{}.add", w),
            BinOp::Sub => format!("{}.sub", w),
            BinOp::Mul => format!("{}.mul", w),
            BinOp::Div if is_float => format!("{}.div", w),
            BinOp::Div => format!("{}.div{}", w, s),
            BinOp::Rem => format!("{}.rem{}", w, s),
            BinOp::Eq => format!("{}.eq", w),
            BinOp::Ne => format!("{}.ne", w),
            BinOp::Lt if is_float => format!("{}.lt", w),
            BinOp::Lt => format!("{}.lt{}", w, s),
            BinOp::Le if is_float => format!("{}.le", w),
            BinOp::Le => format!("{}.le{}", w, s),
            BinOp::Gt if is_float => format!("{}.gt", w),
            BinOp::Gt => format!("{}.gt{}", w, s),
            BinOp::Ge if is_float => format!("{}.ge", w),
            BinOp::Ge => format!("{}.ge{}", w, s),
            BinOp::And | BinOp::BitAnd => format!("{}.and", w),
            BinOp::Or | BinOp::BitOr => format!("{}.or", w),
            BinOp::BitXor => format!("{}.xor", w),
            BinOp::Shl => format!("{}.shl", w),
            BinOp::Shr => format!("{}.shr{}", w, s),
        }
    }
}
