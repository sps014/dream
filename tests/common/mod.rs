//! Shared test harness for the analyzer suite: the parse->analyze->emit pipeline helpers
//! (`compile_test_pipeline` and the `emit_*`/`run_*` wrappers built on it) plus the reusable
//! Dream source stubs (`SYSTEM_STUB`/`JS_STUB`/`ASYNC_STUB`). The `emission_tests` and
//! `analysis_tests` sibling modules `use super::harness::*` to reach these.

#![allow(dead_code)]

use dream::driver::compiler::{Compiler, Target};
use dream::driver::wasm_opt::OptLevel;
use dream::execution::native_c::compile_and_capture;
use dream_diagnostics::DiagnosticBag;
use dream_hir::Hir;
use dream_sema::analyzer::Analyzer;
use dream_syntax::lexer::Lexer;
use dream_syntax::parser::Parser;
use dream_types::TypeInterner;

pub fn analyze_code(code: &str) -> DiagnosticBag {
    let mut diagnostics = DiagnosticBag::new(None);
    let lexer = Lexer::new(code.to_string());
    let arena = bumpalo::Bump::new();
    let mut parser = Parser::new(lexer, &arena, &mut diagnostics);

    if let Ok(tree) = parser.parse() {
        let arena = bumpalo::Bump::new();
        let mut analyzer = Analyzer::new(&tree, &arena);
        let _ = analyzer.analyze(&mut diagnostics);
    }

    diagnostics
}

/// Parses and analyzes `code` (asserting parse + analysis succeed with no diagnostics) and hands the
/// resulting HIR and type interner to `emit`, returning whatever it produces. This is the shared
/// front half — parse -> analyze -> assert clean -> borrow the interner — that every emit helper
/// below otherwise duplicated; each now differs only in the `emit` closure (how it lowers/runs).
pub fn compile_test_pipeline<R>(code: &str, emit: impl FnOnce(&Hir, &TypeInterner) -> R) -> R {
    let mut diagnostics = DiagnosticBag::new(None);
    let lexer = Lexer::new(code.to_string());
    let parse_arena = bumpalo::Bump::new();
    let mut parser = Parser::new(lexer, &parse_arena, &mut diagnostics);
    let tree = parser.parse().expect("parse should succeed");
    let arena = bumpalo::Bump::new();
    let mut analyzer = Analyzer::new(&tree, &arena);
    let hir = analyzer
        .analyze(&mut diagnostics)
        .expect("analysis should succeed")
        .hir;
    assert!(!diagnostics.has_errors(), "unexpected analysis errors");
    let interner = analyzer.interner();
    emit(&hir, interner)
}

/// Analyzes `code`, asserts it is error-free, and runs the *interleaved-emitted* HIR through
/// MIR (`lower -> passes -> emit`), returning the emitted C and how many functions were emitted.
/// Exercises HIR emission end-to-end: source -> analyzer-emitted HIR -> C.
pub fn emit_hir_to_c(code: &str) -> (String, usize) {
    compile_test_pipeline(code, |hir, interner| {
        let count = hir.functions.len();
        let mut mir = dream_mir::lower::lower_program(hir, interner);

        let mut pm = dream_mir::passes::PassManager::new();
        pm.add(dream_mir::passes::CopyConstProp);
        pm.add(dream_mir::passes::ConstFold);
        pm.add(dream_mir::passes::SimplifyCfg);
        pm.add(dream_mir::passes::Dce);
        for f in &mut mir.functions {
            pm.run(f, interner);
        }
        (dream_mir::backend::c::emit_c_module(&mir, interner), count)
    })
}

/// Compiles `code` through the native C backend and returns stdout from `main`.
pub fn run_and_capture(code: &str, _entry: &str) -> String {
    run_native_main(code)
}

pub fn run_and_capture_rc(code: &str, _entry: &str) -> String {
    run_native_main(code)
}

fn run_native_main(code: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("dream_cap_{}_{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let src = dir.join("t.dream");
    // Unit-test HIR stubs duplicate the real prelude; the driver merges stdlib itself.
    let source = [ASYNC_STUB, JS_STUB, CLOSURE_STUB, SYSTEM_STUB]
        .iter()
        .fold(code.to_string(), |s, stub| s.replace(stub, ""));
    let source = dedent_dream_source(&source);
    let source = if source.contains("import system") {
        source
    } else {
        format!("import system;\n{}", source)
    };
    std::fs::write(&src, &source).expect("write source");
    let c = dir.join("t.c");
    let src_s = src.to_string_lossy().into_owned();
    let c_s = c.to_string_lossy().into_owned();
    Compiler::new(Target::NativeC)
        .compile(&src_s, &c_s)
        .unwrap_or_else(|e| panic!("native C compile failed: {}", e));
    compile_and_capture(&c_s, OptLevel::O0).unwrap_or_else(|e| panic!("native C run failed: {}", e))
}

fn dedent_dream_source(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let min = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| {
            if l.len() >= min {
                l[min..].to_string()
            } else {
                (*l).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Like [`emit_hir_to_module`] but runs [`RcInsertion`] first, so `Retain`/`Release` statements are
/// present. Needed to exercise the deep-release runtime: `del()` fires when a reference's last owner
/// is released (here, when a reference local is overwritten). Only RC insertion is run — the
/// optimizing passes are skipped so they cannot elide the release we are testing.
pub fn emit_hir_to_module_rc(code: &str) -> String {
    emit_hir_to_module_rc_only(code)
}

/// Compiles through the production-like MIR pipeline: RC insertion, module optimize (inline), then
/// the default per-function pass manager (includes `RcElision`). Returns full-module C.
pub fn emit_hir_to_module_optimized(code: &str) -> String {
    compile_test_pipeline(code, |hir, interner| {
        let mut mir = dream_mir::lower::lower_program(hir, interner);
        dream_mir::passes::optimize_module(&mut mir, interner);
        let pm = dream_mir::passes::PassManager::default_pipeline();
        for f in &mut mir.functions {
            pm.run(f, interner);
        }
        dream_mir::backend::c::emit_c_module(&mir, interner)
    })
}

/// Same pipeline as [`emit_hir_to_module_optimized`], native C text.
pub fn emit_hir_to_c_optimized(code: &str) -> String {
    compile_test_pipeline(code, |hir, interner| {
        let mut mir = dream_mir::lower::lower_program(hir, interner);
        dream_mir::passes::optimize_module(&mut mir, interner);
        let pm = dream_mir::passes::PassManager::default_pipeline();
        for f in &mut mir.functions {
            pm.run(f, interner);
        }
        dream_mir::backend::c::emit_c_module(&mir, interner)
    })
}

/// Like [`emit_hir_to_c`] but emits the full self-contained module (imports, memory, runtime,
/// exports), so import/scaffold concerns can be asserted and assembled.
pub fn emit_hir_to_module(code: &str) -> String {
    compile_test_pipeline(code, |hir, interner| {
        let mir = dream_mir::lower::lower_program(hir, interner);
        dream_mir::backend::c::emit_c_module(&mir, interner)
    })
}

/// Like [`emit_hir_to_module`] but runs `RcInsertion` first (no other passes), matching the
/// production pipeline where reference-counting is always inserted before emission. Needed for tests
/// that assert on the deep-release runtime: those helper functions are only *reachable* once a
/// release call site references them.
pub fn emit_hir_to_module_rc_only(code: &str) -> String {
    compile_test_pipeline(code, |hir, interner| {
        let mut mir = dream_mir::lower::lower_program(hir, interner);
        use dream_mir::passes::MirPass;
        for f in &mut mir.functions {
            dream_mir::passes::RcInsertion.run(f, interner);
        }
        dream_mir::backend::c::emit_c_module(&mir, interner)
    })
}

/// The `System` intrinsic surface (mirrors `stdlib/system/system.dream`), inlined so the print tests do not
/// depend on the full prelude being merged by the unit-test harness.
pub const SYSTEM_STUB: &str = "
    class System {
        @intrinsic(\"print\")
        static extern fun print<T>(value: T): void;
        @intrinsic(\"println\")
        static extern fun println<T>(value: T): void;
    }
";

/// The dynamic-`js` bridge surface (mirrors `stdlib/core/js.dream`), inlined so the interop tests do
/// not depend on the full prelude being merged by the unit-test harness. `js` itself is a built-in
/// type; these `extend js` declarations provide the entry points and `@js` bridge externs the
/// analyzer desugars dynamic operations into.
pub const JS_STUB: &str = "
    enum Option<T> {
        Some(value: T),
        None,
    }
    extend js {
        @js(\"Dream\", \"jsGlobal\")
        static extern fun global(name: string): js;
        @js(\"Dream\", \"jsGlobalThis\")
        static extern fun global_this(): js;
        @js(\"Dream\", \"jsObject\")
        static extern fun object(): js;
        @js(\"Dream\", \"jsArray\")
        static extern fun array(): js;
        @js(\"Dream\", \"jsFunc\")
        static extern fun func(handler: fun(js): void): js;
        @js(\"Dream\", \"jsFunc0\")
        static extern fun func0(handler: fun(): void): js;
        @js(\"Dream\", \"jsInt\")
        static extern fun box_int(value: int): js;
        @js(\"Dream\", \"jsLong\")
        static extern fun box_long(value: long): js;
        @js(\"Dream\", \"jsDouble\")
        static extern fun box_double(value: double): js;
        @js(\"Dream\", \"jsBool\")
        static extern fun box_bool(value: bool): js;
        @js(\"Dream\", \"jsString\")
        static extern fun box_string(value: string): js;
        @js(\"Dream\", \"jsGetV\")
        static extern fun get(target: js, name: string): js;
        @js(\"Dream\", \"jsSetV\")
        static extern fun set(target: js, name: string, value: js): void;
        @js(\"Dream\", \"jsSetSlot\")
        static extern fun set_slot(target: js, name: string, args_ptr: int, argc: int): void;
        @js(\"Dream\", \"jsCallV\")
        static extern fun call(target: js, name: string, args: js[]): js;
        @js(\"Dream\", \"jsInvokeV\")
        static extern fun invoke(target: js, args: js[]): js;
        @js(\"Dream\", \"jsGetCallV\")
        static extern fun get_call(target: js, prop: string, method: string, args_ptr: int, argc: int): js;
        @js(\"Dream\", \"jsIndexGetV\")
        static extern fun index_get(target: js, key: js): js;
        @js(\"Dream\", \"jsIndexSetV\")
        static extern fun index_set(target: js, key: js, value: js): void;
        @js(\"Dream\", \"jsIndexSetSlot\")
        static extern fun index_set_slot(target: js, args_ptr: int, argc: int): void;
        @js(\"Dream\", \"jsGetAsInt\")
        static extern fun get_as_int(target: js, name: string): int;
        @js(\"Dream\", \"jsGetAsLong\")
        static extern fun get_as_long(target: js, name: string): long;
        @js(\"Dream\", \"jsGetAsDouble\")
        static extern fun get_as_double(target: js, name: string): double;
        @js(\"Dream\", \"jsGetAsBool\")
        static extern fun get_as_bool(target: js, name: string): bool;
        @js(\"Dream\", \"jsGetAsString\")
        static extern fun get_as_string(target: js, name: string): string;
        @js(\"Dream\", \"jsCallAsInt\")
        static extern fun call_as_int(target: js, name: string, args_ptr: int, argc: int): int;
        @js(\"Dream\", \"jsCallAsLong\")
        static extern fun call_as_long(target: js, name: string, args_ptr: int, argc: int): long;
        @js(\"Dream\", \"jsCallAsDouble\")
        static extern fun call_as_double(target: js, name: string, args_ptr: int, argc: int): double;
        @js(\"Dream\", \"jsCallAsBool\")
        static extern fun call_as_bool(target: js, name: string, args_ptr: int, argc: int): bool;
        @js(\"Dream\", \"jsCallAsString\")
        static extern fun call_as_string(target: js, name: string, args_ptr: int, argc: int): string;
        @js(\"Dream\", \"jsInvokeAsInt\")
        static extern fun invoke_as_int(target: js, args_ptr: int, argc: int): int;
        @js(\"Dream\", \"jsInvokeAsLong\")
        static extern fun invoke_as_long(target: js, args_ptr: int, argc: int): long;
        @js(\"Dream\", \"jsInvokeAsDouble\")
        static extern fun invoke_as_double(target: js, args_ptr: int, argc: int): double;
        @js(\"Dream\", \"jsInvokeAsBool\")
        static extern fun invoke_as_bool(target: js, args_ptr: int, argc: int): bool;
        @js(\"Dream\", \"jsInvokeAsString\")
        static extern fun invoke_as_string(target: js, args_ptr: int, argc: int): string;
        @js(\"Dream\", \"jsGetCallAsInt\")
        static extern fun get_call_as_int(target: js, prop: string, method: string, args_ptr: int, argc: int): int;
        @js(\"Dream\", \"jsGetCallAsLong\")
        static extern fun get_call_as_long(target: js, prop: string, method: string, args_ptr: int, argc: int): long;
        @js(\"Dream\", \"jsGetCallAsDouble\")
        static extern fun get_call_as_double(target: js, prop: string, method: string, args_ptr: int, argc: int): double;
        @js(\"Dream\", \"jsGetCallAsBool\")
        static extern fun get_call_as_bool(target: js, prop: string, method: string, args_ptr: int, argc: int): bool;
        @js(\"Dream\", \"jsGetCallAsString\")
        static extern fun get_call_as_string(target: js, prop: string, method: string, args_ptr: int, argc: int): string;
        @js(\"Dream\", \"jsAwait\")
        static extern async fun await_promise(target: js): js;
        @js(\"Dream\", \"jsAsInt\")
        static extern fun as_int(target: js): int;
        @js(\"Dream\", \"jsAsLong\")
        static extern fun as_long(target: js): long;
        @js(\"Dream\", \"jsAsDouble\")
        static extern fun as_double(target: js): double;
        @js(\"Dream\", \"jsAsBool\")
        static extern fun as_bool(target: js): bool;
        @js(\"Dream\", \"jsAsString\")
        static extern fun as_string(target: js): string;
        public fun to_int(): int { return js.as_int(this); }
        public fun to_str(): string { return js.as_string(this); }
    }
";

/// The closure ABI intrinsics (mirrors `stdlib/core/closure.dream`), inlined so `fun(...)`-value
/// tests do not depend on the full prelude being merged by the unit-test harness. Every `fun(...)`
/// value is boxed through `Closure.funcbox_new`/`funcbox_funcidx`/`funcbox_env` (see
/// `hir_set_func_value`/`hir_set_indirect_call`), so any test exercising a function value or an
/// indirect call needs this merged in alongside its own code.
pub const CLOSURE_STUB: &str = "
    class Closure {
        @intrinsic(\"funcbox_new\")
        static extern fun funcbox_new(funcidx: int, env: int): int;
        @intrinsic(\"funcbox_funcidx\")
        static extern fun funcbox_funcidx(box: int): int;
        @intrinsic(\"funcbox_env\")
        static extern fun funcbox_env(box: int): int;
        @intrinsic(\"retain\")
        static extern fun retain(v: object): void;
    }
    class CaptureCell<T> {
        public value: T;
        public constructor(v: T) {
            this.value = v;
        }
    }
";

/// `System` + `Time.sleep` for async tests (mirrors `stdlib/system/time.dream` + `system.dream`).
pub const ASYNC_STUB: &str = "
    class System {
        @intrinsic(\"print\")
        static extern fun print<T>(value: T): void;
        @intrinsic(\"println\")
        static extern fun println<T>(value: T): void;
    }
    class Time {
        @intrinsic(\"sleep\")
        static extern async fun sleep(ms: int): void;
    }
";
