//! The program entry point: `dream_guest_entry` (plus native `main`), and the machinery that turns
//! `main`'s return value into a process exit status.
//!
//! `main(): void` always exits 0, `main(): int` returns the code directly, and
//! `main(): Result<T, E>` prints `Error: <e>` on stderr and exits 1. The `Result` case is emitted as
//! a standalone status helper rather than inline, because wasm32 cannot compute it in the entry: an
//! async `main` returns its still-pending Future to the JS host, which only calls back once the
//! Future settles (see [`emit_main_report`]).

use super::ast::{CTy, Expr, Item, Stmt};
use super::builder::{FuncBuilder, ModuleBuilder};
use super::ctx::Cx;
use super::release::release_sym;
use super::rvalue::{to_string_fn, union_discriminant, union_field};
use crate::abi::FutureLayout;
use crate::MirFunction;
use dream_types::{PrimTy, TyKind, TypeId};

/// The `Err` line's prefix, matching Rust's `Error: {err:?}`. Interned like any other literal.
pub(super) const ERROR_PREFIX: &str = "Error: ";

/// Module-level slot holding the exit status once `main` has finished. Native returns the status
/// directly; wasm32 reads it back through [`crate::abi::EXPORT_MAIN_REPORT`].
const RC_SLOT: &str = "__dream_main_rc";

/// The status helper generated for a `Result`-returning `main`.
const STATUS_FN: &str = "__dream_main_status";

/// How `main`'s declared return type becomes the process exit status.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EntryExit {
    /// `main(): void` — always 0.
    Void,
    /// `main(): int` — the returned value is the exit code.
    Code,
    /// `main(): Result<T, E>` — `Err` reports on stderr and exits 1. Carries the `Result` type.
    Report(TypeId),
}

/// Classifies `main`'s return type. Sema has already rejected every other spelling, so anything
/// unrecognized here is treated as `void` rather than diagnosed.
pub(super) fn entry_exit(main: &MirFunction, cx: &Cx<'_>) -> EntryExit {
    // `MirFunction::ret` is the *declared* type even for `async fun`, so the async stub and the
    // sync body classify identically.
    match cx.interner.kind(main.ret) {
        TyKind::Prim(PrimTy::Int) => EntryExit::Code,
        TyKind::Union(..) if cx.nunion(main.ret).is_some() => EntryExit::Report(main.ret),
        _ => EntryExit::Void,
    }
}

/// True when `main` returns a `Result`, so the module needs the `Error:` literal. Answerable from
/// the MIR alone, because string interning runs while the [`Cx`] is still being built.
pub(super) fn entry_reports_error(mir: &crate::Mir) -> bool {
    mir.functions
        .iter()
        .any(|f| f.name == crate::abi::ENTRY_FN && mir.layouts.unions.contains_key(&f.ret))
}

/// The error payload type of a `Result`-returning `main`, for callers that must pre-register work
/// keyed on it — `to_string` protocol reachability in particular, which is computed from MIR
/// statements and would otherwise never see this hand-emitted call site.
pub(super) fn entry_error_type(cx: &Cx<'_>) -> Option<TypeId> {
    let main = cx
        .mir
        .functions
        .iter()
        .find(|f| f.name == crate::abi::ENTRY_FN)?;
    let EntryExit::Report(ty) = entry_exit(main, cx) else {
        return None;
    };
    err_variant(cx, ty).map(|(_, field_ty)| field_ty)
}

/// `(discriminant, payload type)` of the `Result`'s failure variant.
fn err_variant(cx: &Cx<'_>, ty: TypeId) -> Option<(usize, TypeId)> {
    let u = cx.nunion(ty)?;
    let v = u.variants.iter().find(|v| v.name == "Err")?;
    let field = v.fields.first()?;
    Some((v.discriminant as usize, field.ty))
}

pub(super) fn emit_guest_entry(
    m: &mut ModuleBuilder,
    cx: &Cx<'_>,
    main: &MirFunction,
    async_n: usize,
) {
    let exit = entry_exit(main, cx);
    if exit != EntryExit::Void {
        m.push(Item::Global {
            thread_local: false,
            align: None,
            static_: true,
            const_: false,
            ty: CTy::I32,
            name: RC_SLOT.into(),
            init: Some(Expr::i(0)),
        });
    }
    if let EntryExit::Report(ty) = exit {
        emit_status_fn(m, cx, ty);
    }

    let mut entry = FuncBuilder::new(CTy::I32, crate::abi::GUEST_ENTRY_FN);
    if cx.target.is_wasm32() {
        entry.export = Some(crate::abi::ENTRY_FN.to_string());
    }
    entry.call("dream_runtime_init", vec![]);
    let main_args = if main.params.is_empty() {
        vec![]
    } else {
        vec![Expr::call("dream_array_new", vec![Expr::i(0), Expr::i(8)])]
    };
    if main.is_async {
        entry.stmt(Stmt::decl(
            CTy::Ptr,
            "__mf",
            Some(Expr::call("main_dream", main_args)),
        ));
        // Futures are lazy; the entry point launches async main explicitly.
        entry.call("dream_start", vec![Expr::id("__mf")]);
    } else if exit == EntryExit::Void {
        entry.call("main_dream", main_args);
    } else {
        entry.stmt(Stmt::decl(
            main_value_cty(exit),
            "__mv",
            Some(Expr::call("main_dream", main_args)),
        ));
    }
    if async_n > 0 {
        entry.call("dream_run_loop", vec![]);
    }
    if cx.mir.uses_defer {
        entry.call("dream_defer_drain_all", vec![]);
    }
    // A sync `main` has already produced its value; an async one has it in the settled Future,
    // except on wasm32 where the loop has not run yet and the host reports later.
    let settled = if main.is_async {
        (!cx.target.is_wasm32()).then(|| future_result(cx, exit, Expr::id("__mf")))
    } else {
        Some(Expr::id("__mv"))
    };
    if exit != EntryExit::Void {
        if let Some(value) = settled {
            entry.stmt(Stmt::assign(Expr::id(RC_SLOT), status_of(exit, value)));
        }
    }
    if main.is_async && !cx.target.is_wasm32() {
        // Wasm32 returns the Future to the JS host (`Instance.run`). Native owns it.
        entry.call("dream_release", vec![Expr::id("__mf")]);
    }
    if !cx.target.is_wasm32() {
        entry.call("dream_drop_globals", vec![]);
    }
    if cx.target.is_wasm32() && main.is_async {
        entry.ret(Some(Expr::cast(CTy::I32, Expr::id("__mf"))));
    } else if cx.target.is_wasm32() || exit == EntryExit::Void {
        // The wasm32 entry's return slot means "0, or a Future pointer" — never an exit code.
        entry.ret(Some(Expr::i(0)));
    } else {
        entry.ret(Some(Expr::id(RC_SLOT)));
    }
    m.push_func(entry);

    if cx.target.is_wasm32() {
        emit_main_report(m, cx, main, exit);
        return;
    }
    emit_native_main(m, cx);
}

/// C type of `main_dream`'s return value.
fn main_value_cty(exit: EntryExit) -> CTy {
    match exit {
        // `main(): int` returns `int32_t` (`c_ty` / `local_c_ty`).
        EntryExit::Code => CTy::I32,
        _ => CTy::Ptr,
    }
}

/// Reads the settled value out of an async `main`'s Future frame.
fn future_result(cx: &Cx<'_>, exit: EntryExit, fut: Expr) -> Expr {
    let layout = if cx.target.is_wasm32() {
        FutureLayout::WASM32
    } else {
        FutureLayout::native()
    };
    let slot = Expr::ptr_add(fut, Expr::i(layout.result as i64));
    Expr::load(main_value_cty(exit), slot)
}

/// The exit status for a finished `main` value.
fn status_of(exit: EntryExit, value: Expr) -> Expr {
    match exit {
        EntryExit::Void => Expr::i(0),
        EntryExit::Code => Expr::cast(CTy::I32, value),
        EntryExit::Report(_) => Expr::call(STATUS_FN, vec![value]),
    }
}

/// `static int32_t __dream_main_status(void *v)` — reports an `Err` on stderr, releases the
/// `Result`, and answers the exit status.
fn emit_status_fn(m: &mut ModuleBuilder, cx: &Cx<'_>, ty: TypeId) {
    let mut b = FuncBuilder::new(CTy::I32, STATUS_FN);
    b.static_ = true;
    b.param(CTy::Ptr, "v");
    b.stmt(Stmt::decl(CTy::I32, "rc", Some(Expr::i(0))));

    if let Some((err_disc, payload_ty)) = err_variant(cx, ty) {
        let payload = union_field(cx, ty, err_disc, 0, Expr::id("v"));
        let conv = to_string_fn(cx, payload_ty);
        let mut report = vec![Stmt::call(
            "print_err_string",
            vec![Expr::id(cx.str_sym(ERROR_PREFIX))],
        )];
        if conv.is_empty() {
            // `E` is already a `string`; the union release owns it.
            report.push(Stmt::call("print_err_string", vec![payload]));
        } else {
            report.push(Stmt::decl(
                CTy::Ptr,
                "__t",
                Some(Expr::call(conv, vec![payload])),
            ));
            report.push(Stmt::call("print_err_string", vec![Expr::id("__t")]));
            report.push(Stmt::call("dream_release", vec![Expr::id("__t")]));
        }
        report.push(Stmt::call("print_err_char", vec![Expr::i(10)]));
        report.push(Stmt::assign(Expr::id("rc"), Expr::i(1)));
        b.stmt(Stmt::if_(
            Expr::eq(
                union_discriminant(cx, ty, Expr::id("v")),
                Expr::i(err_disc as i64),
            ),
            Stmt::block(report),
        ));
    }
    b.stmt(Stmt::call(release_sym(cx, ty), vec![Expr::id("v")]));
    b.ret(Some(Expr::id("rc")));
    m.push_func(b);
}

/// wasm32 only: the host calls this once `main` has finished to collect the exit status. For an
/// async `main` it is handed the settled Future (the entry returned before the loop ran); for a
/// sync one it is handed null and just reads back what the entry already computed.
fn emit_main_report(m: &mut ModuleBuilder, cx: &Cx<'_>, main: &MirFunction, exit: EntryExit) {
    if exit == EntryExit::Void {
        return;
    }
    let mut b = FuncBuilder::new(CTy::I32, crate::abi::EXPORT_MAIN_REPORT);
    b.export = Some(crate::abi::EXPORT_MAIN_REPORT.to_string());
    b.param(CTy::Ptr, "fut");
    if main.is_async {
        b.stmt(Stmt::if_(
            Expr::id("fut"),
            Stmt::assign(
                Expr::id(RC_SLOT),
                status_of(exit, future_result(cx, exit, Expr::id("fut"))),
            ),
        ));
    } else {
        b.stmt(Stmt::expr(Expr::cast(CTy::Void, Expr::id("fut"))));
    }
    b.ret(Some(Expr::id(RC_SLOT)));
    m.push_func(b);
}

fn emit_native_main(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    let mut main_fn = FuncBuilder::new(CTy::I32, "main");
    main_fn.param(CTy::I32, "argc");
    main_fn.param(CTy::ptr_to(CTy::CharPtr), "argv");
    main_fn.call(
        "dream_process_capture_args",
        vec![Expr::id("argc"), Expr::id("argv")],
    );
    let rc = main_fn.temp(
        CTy::I32,
        Some(Expr::call(crate::abi::GUEST_ENTRY_FN, vec![])),
    );
    // Heap-counter leak report. Debug builds always print it so `dream run` / `-g` show
    // retention; release builds opt in via DREAM_DEBUG_LEAKS=1. Counters themselves always
    // update so `Debug.live_objects()` is valid in `--release` goldens.
    let leak_report = Stmt::block(vec![
        Stmt::call(
            "fprintf",
            vec![
                Expr::id("stderr"),
                Expr::cstr("[dream] leak check: live=%d total_allocations=%d\n"),
                Expr::call("debug_get_live_objects", vec![]),
                Expr::call("debug_get_total_allocations", vec![]),
            ],
        ),
        Stmt::call("debug_dump_live", vec![]),
    ]);
    if cx.leak_checks {
        main_fn.stmt(leak_report);
    } else {
        main_fn.stmt(Stmt::if_(
            Expr::ne(
                Expr::call("getenv", vec![Expr::cstr("DREAM_DEBUG_LEAKS")]),
                Expr::cast(CTy::Ptr, Expr::i(0)),
            ),
            leak_report,
        ));
    }
    main_fn.ret(Some(rc));
    m.push_func(main_fn);
}
