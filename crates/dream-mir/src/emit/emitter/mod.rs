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

use super::builder::{
    BlockTy, ExtractLane, FuncBuilder, Label, LoadKind, ModuleBuilder, Nullary, ReplaceLane,
    StoreKind, ValType,
};
use super::*;
use crate::async_emit::{AsyncSlots, F_AWAITING, F_RESULT, F_STATE, F_WIDE};
use crate::emit::valuetype::{is_simd_vector, ValueFrame, ValueLocalKind};
use dream_abi::intrinsics::IntrinsicOp;
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
    finish_func_wat(emit_function_with(
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
        &HashMap::new(),
    ))
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
    intrinsics: &HashMap<DefId, IntrinsicOp>,
) -> FuncBuilder {
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
        intrinsics,
    );
    e.emit();
    e.f
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
    intrinsics: &HashMap<DefId, IntrinsicOp>,
) -> FuncBuilder {
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
        intrinsics,
    );
    e.emit_async_state_machine(slots, poll_sym);
    e.f
}

struct Emitter<'a> {
    func: &'a MirFunction,
    interner: &'a TypeInterner,
    symbols: &'a HashMap<(DefId, Vec<TypeId>), String>,
    /// `@intrinsic` ops keyed by callee `DefId` (including monomorphized Vector methods).
    intrinsics: &'a HashMap<DefId, IntrinsicOp>,
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
    f: FuncBuilder,
    /// When emitting inside an async poll segment, the enclosing task (for scope-exit release).
    async_parent: Option<&'a MirFunction>,
    /// In an async poll body, the count of persistent user locals (params + declared `let`s) at the
    /// front of `func.locals`; only these get value(`struct`) drop glue on completion. RC locals are
    /// released by MIR `Release` stmts from poll `RcInsertion`. Synthetic temps that follow are
    /// transient (their RC is still MIR-managed when owned).
    async_user_locals: usize,
    /// Generate `@name` annotations (binary emit uses the name section instead).
    _debug: bool,
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
        intrinsics: &'a HashMap<DefId, IntrinsicOp>,
    ) -> Self {
        let frame = ValueFrame::compute(func, interner, layouts);
        Emitter {
            func,
            interner,
            symbols,
            intrinsics,
            sigs,
            layouts,
            strings,
            tags,
            func_table,
            defined_funcs,
            value_glue,
            global_tys,
            frame,
            f: FuncBuilder::new(func_symbol(func)),
            async_parent,
            async_user_locals,
            _debug: debug,
            locate_panics,
            debug_fn,
            current_line: 0,
            shape_scopes: Vec::new(),
            shape_label_id: 0,
        }
    }
}

impl Emitter<'_> {
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
        if self.returns_value_struct() {
            self.f.param("__sret", ValType::I32);
        }
        for p in &self.func.params {
            let ty = if self.is_v128_local(*p) {
                ValType::V128
            } else {
                wasm_val_ty(self.interner, self.func.local_ty(*p))
            };
            self.f.param(&p.0.to_string(), ty);
        }
        if self.returns_value_struct() {
            // sret is the leading param; no WASM result.
        } else if !matches!(self.interner.kind(self.func.ret), TyKind::Void) {
            self.f.result(wasm_val_ty(self.interner, self.func.ret));
        }

        let param_count = self.func.params.len();
        for (i, decl) in self.func.locals.iter().enumerate() {
            if i < param_count {
                continue;
            }
            let ty = if is_simd_vector(self.layouts, decl.ty) {
                ValType::V128
            } else {
                wasm_val_ty(self.interner, decl.ty)
            };
            self.f.local(&i.to_string(), ty);
        }
        self.f.local("__pc", ValType::I32);
        if self.frame.size > 0 {
            self.f.local("__saved_sp", ValType::I32);
        }
        self.f.local("__obj", ValType::I32);
        self.f.local("__len", ValType::I32);
        self.f.local("__old_len", ValType::I32);
        self.f.local("__rel", ValType::I32);
        self.f.local("__jsp", ValType::I32);
        self.f.local("__wsrc", ValType::I32);
        self.f.local("__wbox", ValType::I32);
        self.f.local("__src", ValType::I32);
        self.f.local("__v128", ValType::V128);

        self.emit_value_frame_prologue();
        if let Some(dbg) = self.debug_fn {
            self.f.i32_const(dbg.id as i32);
            self.f.call("__dbg_enter");
        }
        self.emit_shaped_body();
    }

    /// Emits the `dream_debug.exit` hook (pops the debugger's call-stack frame) right before a return,
    /// when debug-info is on. Placed at every real return site so the shadow call stack stays
    /// balanced regardless of which path exits the function.
    fn emit_debug_exit(&mut self) {
        if let Some(dbg) = self.debug_fn {
            self.f.i32_const((dbg.id) as i32);
            self.f.call("__dbg_exit");
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
        self.f.i32_const((self.string_addr(&msg)) as i32);
        self.f.call("dream_panic");
    }

    /// Traps if the just-loaded `unowned` field value (top of stack) is the poison sentinel `0`
    /// (written by `$weak_clear_all` when the referent was freed — see `src/mir/runtime/weak.wat`),
    /// otherwise leaves the value back on the stack unchanged. Runtime semantics for `unowned`: see
    /// `docs/language/memory.md`.
    fn emit_unowned_read_check(&mut self) {
        self.f.local_set("__wsrc");
        self.f.local_get("__wsrc");
        self.f.i32_eqz();
        self.f.if_();
        self.emit_panic(super::panic_msgs::UNOWNED_NULL_DEREF);
        self.f.end();
        self.f.local_get("__wsrc");
    }

    /// Pushes the address of `base.field` (`base + offset`) onto the stack.
    fn field_addr(&mut self, base: crate::Local, offset: u32) {
        self.f.local_get(&(base.0).to_string());
        if offset > 0 {
            self.f.i32_const((offset) as i32);
            self.f.i32_add();
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
        self.f.load(LoadKind::I32, 0);
        self.f.i32_ge_u();
        self.f.if_();
        self.emit_panic(super::panic_msgs::INDEX_OUT_OF_BOUNDS);
        self.f.end();
    }

    /// Pushes the address of `base[index]` (`base + 4 + index * elem_size`) onto the stack. The
    /// length occupies the first word, so element 0 is at offset 4. Checked: traps via `$dream_panic`
    /// if `index` is out of range (see [`Self::emit_bounds_check`]).
    fn elem_addr(&mut self, base: crate::Local, elem_ty: TypeId, index: &Operand, unchecked: bool) {
        let (size, _) = scalar_size(self.interner, elem_ty);
        if !unchecked {
            self.emit_bounds_check(|s| s.f.local_get(&(base.0).to_string()), index);
        }
        self.f.local_get(&(base.0).to_string());
        self.f.i32_const(4);
        self.f.i32_add();
        self.emit_operand(index);
        self.f.i32_const((size) as i32);
        self.f.i32_mul();
        self.f.i32_add();
    }

    /// Emits `s[i]` as an inlined UTF-16 unit load. When `unchecked` is false, a located
    /// `index >= unit_len` check runs first (precise file:line panic).
    fn emit_char_at(&mut self, s: &Operand, i: &Operand, unchecked: bool) {
        if !unchecked {
            self.emit_operand(i);
            self.emit_operand(s);
            self.f.load(LoadKind::I32, 0);
            self.f.i32_ge_u();
            self.f.if_();
            self.emit_panic(super::panic_msgs::INDEX_OUT_OF_BOUNDS);
            self.f.end();
        }
        self.emit_operand(s);
        self.f.i32_const(crate::abi::STRING_UTF8_OFFSET as i32);
        self.f.i32_add();
        self.emit_operand(i);
        self.f.i32_const(1);
        self.f.i32_shl();
        self.f.i32_add();
        self.f.load(LoadKind::I32_16U, 0);
    }

    /// Emits `s.byte_at(i)` as an inlined payload byte load, with an optional byte-length check.
    fn emit_byte_at(&mut self, s: &Operand, i: &Operand, unchecked: bool) {
        if !unchecked {
            self.emit_operand(i);
            self.emit_operand(s);
            self.f.load(LoadKind::I32, 0);
            self.f.i32_const(1);
            self.f.i32_shl();
            self.f.i32_ge_u();
            self.f.if_();
            self.emit_panic(super::panic_msgs::INDEX_OUT_OF_BOUNDS);
            self.f.end();
        }
        self.emit_operand(s);
        self.f.i32_const(crate::abi::STRING_UTF8_OFFSET as i32);
        self.f.i32_add();
        self.emit_operand(i);
        self.f.i32_add();
        self.f.load(LoadKind::I32_8U, 0);
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

    fn load_kind(&self, ty: TypeId) -> LoadKind {
        load_kind_for(self.interner, ty)
    }

    fn store_kind(&self, ty: TypeId) -> StoreKind {
        store_kind_for(self.interner, ty)
    }

    fn emit_operand(&mut self, op: &Operand) {
        match op {
            Operand::Const(c) => self.emit_const(c),
            Operand::Copy(Place::Local(l)) => self.f.local_get(&(l.0).to_string()),
            Operand::Copy(Place::Global(g)) => self.f.global_get(&format!("g{}", g.0)),
            Operand::Copy(Place::Field { base, field }) => {
                if self.is_v128_local(*base) {
                    let lane = self
                        .layouts
                        .get(self.func.local_ty(*base))
                        .and_then(|l| l.fields.get(*field))
                        .map(|f| f.offset / 4)
                        .unwrap_or(0);
                    self.f.local_get(&(base.0).to_string());
                    self.f.extract_lane(ExtractLane::I32x4, lane as u8);
                    return;
                }
                if let Some(f) = self.field_layout_full(*base, *field) {
                    let (off, fty, is_unowned) = (f.offset, f.ty, f.is_unowned);
                    self.field_addr(*base, off);
                    // A value-struct field is addressed inline, not loaded: reading it yields the
                    // address of its inline storage (the consumer copies where a value is needed).
                    if !self.interner.is_value_type(fty) {
                        self.f.load(self.load_kind(fty), 0);
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
                        self.f.load(self.load_kind(ety), 0);
                    }
                } else {
                    crate::internal_error!("missing array element type for read (base {:?})", base);
                }
            }
            Operand::Copy(Place::Deref { ptr, elem_ty }) => {
                self.f.local_get(&(ptr.0).to_string());
                if !self.interner.is_value_type(*elem_ty) {
                    self.f.load(self.load_kind(*elem_ty), 0);
                }
            }
        }
    }

    fn emit_const(&mut self, c: &Const) {
        match c {
            Const::Int(v) => self.f.i32_const(*v as i32),
            Const::Long(v) => self.f.i64_const(*v),
            Const::Float(v) => self.f.f64_const(*v),
            Const::F32(v) => self.f.f32_const(*v),
            Const::Bool(v) => self.f.i32_const(*v as i32),
            Const::Char(v) => self.f.i32_const(*v as i32),
            Const::Null => self.f.i32_const(0),
            Const::Str(s) => match self.strings.get(s) {
                Some(addr) => self.f.i32_const(*addr as i32),
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

    fn is_v128_local(&self, l: crate::Local) -> bool {
        let i = l.0 as usize;
        if i < self.func.params.len() {
            return false;
        }
        let decl = &self.func.locals[i];
        is_simd_vector(self.layouts, decl.ty)
    }

    fn emit_v128_spill_addr(&mut self) {
        self.emit_frame_off(self.frame.v128_spill.unwrap_or(0));
    }

    fn emit_frame_off(&mut self, off: u32) {
        self.f.global_get("__sp");
        if off > 0 {
            self.f.i32_const(off as i32);
            self.f.i32_add();
        }
    }

    fn emit_v128_slot_addr(&mut self, l: crate::Local) {
        if let Some(off) = self.frame.v128_slot(l) {
            self.emit_frame_off(off);
        } else {
            self.emit_v128_spill_addr();
        }
    }

    /// Spills a `v128` local to its shadow slot and leaves the slot address on the stack (value-struct
    /// pointer ABI for callees that were not SIMD-lowered).
    pub(super) fn emit_v128_as_ptr(&mut self, l: crate::Local) {
        self.emit_v128_slot_addr(l);
        self.f.local_get(&(l.0).to_string());
        self.f.store(StoreKind::V128, 0);
        self.emit_v128_slot_addr(l);
    }

    fn emit_v128_sret_into_local(&mut self, dest: u32, callee: &crate::Callee, args: &[Operand]) {
        let dest_l = crate::Local(dest);
        self.emit_v128_slot_addr(dest_l);
        self.emit_call_args(callee, args);
        self.f.call(&self.callee_symbol(callee));
        self.emit_v128_slot_addr(dest_l);
        self.f.load(LoadKind::V128, 0);
        self.f.local_set(&dest.to_string());
    }

    fn binop_instr(&self, op: BinOp, ty: TypeId) -> Nullary {
        let signed = !matches!(
            self.interner.kind(ty),
            TyKind::Prim(PrimTy::UInt | PrimTy::ULong | PrimTy::Byte)
        );
        match (wasm_val_ty(self.interner, ty), op, signed) {
            (ValType::I32, BinOp::Add, _) => Nullary::I32Add,
            (ValType::I32, BinOp::Sub, _) => Nullary::I32Sub,
            (ValType::I32, BinOp::Mul, _) => Nullary::I32Mul,
            (ValType::I32, BinOp::Div, true) => Nullary::I32DivS,
            (ValType::I32, BinOp::Div, false) => Nullary::I32DivU,
            (ValType::I32, BinOp::Rem, true) => Nullary::I32RemS,
            (ValType::I32, BinOp::Rem, false) => Nullary::I32RemU,
            (ValType::I32, BinOp::Eq, _) => Nullary::I32Eq,
            (ValType::I32, BinOp::Ne, _) => Nullary::I32Ne,
            (ValType::I32, BinOp::Lt, true) => Nullary::I32LtS,
            (ValType::I32, BinOp::Lt, false) => Nullary::I32LtU,
            (ValType::I32, BinOp::Le, true) => Nullary::I32LeS,
            (ValType::I32, BinOp::Le, false) => Nullary::I32LeU,
            (ValType::I32, BinOp::Gt, true) => Nullary::I32GtS,
            (ValType::I32, BinOp::Gt, false) => Nullary::I32GtU,
            (ValType::I32, BinOp::Ge, true) => Nullary::I32GeS,
            (ValType::I32, BinOp::Ge, false) => Nullary::I32GeU,
            (ValType::I32, BinOp::And | BinOp::BitAnd, _) => Nullary::I32And,
            (ValType::I32, BinOp::Or | BinOp::BitOr, _) => Nullary::I32Or,
            (ValType::I32, BinOp::BitXor, _) => Nullary::I32Xor,
            (ValType::I32, BinOp::Shl, _) => Nullary::I32Shl,
            (ValType::I32, BinOp::Shr, true) => Nullary::I32ShrS,
            (ValType::I32, BinOp::Shr, false) => Nullary::I32ShrU,
            (ValType::I64, BinOp::Add, _) => Nullary::I64Add,
            (ValType::I64, BinOp::Sub, _) => Nullary::I64Sub,
            (ValType::I64, BinOp::Mul, _) => Nullary::I64Mul,
            (ValType::I64, BinOp::Div, true) => Nullary::I64DivS,
            (ValType::I64, BinOp::Div, false) => Nullary::I64DivU,
            (ValType::I64, BinOp::Rem, true) => Nullary::I64RemS,
            (ValType::I64, BinOp::Rem, false) => Nullary::I64RemU,
            (ValType::I64, BinOp::Eq, _) => Nullary::I64Eq,
            (ValType::I64, BinOp::Ne, _) => Nullary::I64Ne,
            (ValType::I64, BinOp::Lt, true) => Nullary::I64LtS,
            (ValType::I64, BinOp::Lt, false) => Nullary::I64LtU,
            (ValType::I64, BinOp::Le, true) => Nullary::I64LeS,
            (ValType::I64, BinOp::Le, false) => Nullary::I64LeU,
            (ValType::I64, BinOp::Gt, true) => Nullary::I64GtS,
            (ValType::I64, BinOp::Gt, false) => Nullary::I64GtU,
            (ValType::I64, BinOp::Ge, true) => Nullary::I64GeS,
            (ValType::I64, BinOp::Ge, false) => Nullary::I64GeU,
            (ValType::I64, BinOp::And | BinOp::BitAnd, _) => Nullary::I64And,
            (ValType::I64, BinOp::Or | BinOp::BitOr, _) => Nullary::I64Or,
            (ValType::I64, BinOp::BitXor, _) => Nullary::I64Xor,
            (ValType::I64, BinOp::Shl, _) => Nullary::I64Shl,
            (ValType::I64, BinOp::Shr, true) => Nullary::I64ShrS,
            (ValType::I64, BinOp::Shr, false) => Nullary::I64ShrU,
            (ValType::F32, BinOp::Add, _) => Nullary::F32Add,
            (ValType::F32, BinOp::Sub, _) => Nullary::F32Sub,
            (ValType::F32, BinOp::Mul, _) => Nullary::F32Mul,
            (ValType::F32, BinOp::Div, _) => Nullary::F32Div,
            (ValType::F32, BinOp::Eq, _) => Nullary::F32Eq,
            (ValType::F32, BinOp::Ne, _) => Nullary::F32Ne,
            (ValType::F32, BinOp::Lt, _) => Nullary::F32Lt,
            (ValType::F32, BinOp::Le, _) => Nullary::F32Le,
            (ValType::F32, BinOp::Gt, _) => Nullary::F32Gt,
            (ValType::F32, BinOp::Ge, _) => Nullary::F32Ge,
            (ValType::F64, BinOp::Add, _) => Nullary::F64Add,
            (ValType::F64, BinOp::Sub, _) => Nullary::F64Sub,
            (ValType::F64, BinOp::Mul, _) => Nullary::F64Mul,
            (ValType::F64, BinOp::Div, _) => Nullary::F64Div,
            (ValType::F64, BinOp::Eq, _) => Nullary::F64Eq,
            (ValType::F64, BinOp::Ne, _) => Nullary::F64Ne,
            (ValType::F64, BinOp::Lt, _) => Nullary::F64Lt,
            (ValType::F64, BinOp::Le, _) => Nullary::F64Le,
            (ValType::F64, BinOp::Gt, _) => Nullary::F64Gt,
            (ValType::F64, BinOp::Ge, _) => Nullary::F64Ge,
            _ => crate::internal_error!("no WASM binop for {op:?} on {ty:?}"),
        }
    }
}

fn finish_func_wat(f: FuncBuilder) -> String {
    let mut m = ModuleBuilder::new();
    for g in f.global_names() {
        m.global_i32(&g, true, 0);
    }
    for ty in f.type_names_used() {
        m.intern_type(Some(&ty), vec![ValType::I32], vec![ValType::I32]);
    }
    m.table("__ft", 1, 1);
    let name = f.name().to_string();
    for c in f.callees() {
        if c != name {
            m.import_func("env", &c, &c, vec![ValType::I32], vec![ValType::I32]);
        }
    }
    m.push_func(f);
    m.finish_wat()
}
