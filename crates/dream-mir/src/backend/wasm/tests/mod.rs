use super::*;
use crate::build::FunctionBuilder;
use crate::{Place, Rvalue, Terminator};

#[test]
fn module_wraps_and_resolves_call_symbols() {
    use crate::Callee;
    use dream_types::DefId;
    let i = TypeInterner::new();

    // fun callee(): int { return 0; }  (def 1)
    let mut cb = FunctionBuilder::new("callee", i.int());
    cb.set_def(DefId(1), vec![]);
    cb.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
    let callee = cb.finish();

    // fun caller(): int { return callee(); }  (def 2, calls def 1)
    let mut rb = FunctionBuilder::new("caller", i.int());
    rb.set_def(DefId(2), vec![]);
    let t = rb.new_temp(i.int());
    rb.assign(
        Place::Local(t),
        Rvalue::Call {
            callee: Callee {
                def: DefId(1),
                args: vec![],
                ret: i.int(),
                take_params: vec![],
            },
            args: vec![],
        },
    );
    rb.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
    let caller = rb.finish();

    let mir = crate::Mir {
        functions: vec![callee, caller],
        ..Default::default()
    };
    let wat = emit_module(&mir, &i, false);
    assert!(
        wat.starts_with("(module"),
        "should be wrapped in a module:\n{}",
        wat
    );
    assert!(wat.contains("(func $callee"), "callee header:\n{}", wat);
    // The call site resolves to the callee's symbol, not a bare def index.
    assert!(
        wat.contains("call $callee"),
        "call must resolve to the header symbol:\n{}",
        wat
    );
    assert!(
        wat.contains("(export \"caller\""),
        "non-instance funcs are exported:\n{}",
        wat
    );
}

#[test]
fn instance_functions_get_distinct_symbols() {
    use dream_types::DefId;
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("id", i.int());
    b.set_def(DefId(7), vec![i.int()]);
    b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
    let f = b.finish();
    let wat = emit_function(&f, &i);
    // The instance args are encoded into the symbol so monomorphizations stay distinct.
    assert!(
        wat.contains(&format!("(func $id__{}", i.int().0)),
        "instance symbol:\n{}",
        wat
    );
}

#[test]
fn field_access_uses_layout_offsets_and_widths() {
    use dream_hir::{FieldLayout, LayoutTable, TypeLayout};
    use dream_types::DefId;
    let mut i = TypeInterner::new();
    let def = DefId(3);
    let dbl = i.prim(PrimTy::Double);
    let int = i.int();
    let sty = i.struct_ty(def, vec![]);

    let mut layouts = LayoutTable::default();
    layouts.insert(
        sty,
        TypeLayout {
            name: "S".into(),
            fields: vec![
                FieldLayout {
                    offset: 0,
                    ty: int,
                    name: "a".into(),
                    is_weak: false,
                    is_unowned: false,
                },
                FieldLayout {
                    offset: 8,
                    ty: dbl,
                    name: "b".into(),
                    is_weak: false,
                    is_unowned: false,
                },
            ],
            size: 16,
            packed: false,
        },
    );

    // fun read(p: S): double { return p.<field 1>; }
    let mut b = FunctionBuilder::new("read", dbl);
    b.set_def(DefId(9), vec![]);
    let p = b.new_param(sty, Some("p".into()));
    let t = b.new_temp(dbl);
    b.assign(
        Place::Local(t),
        Rvalue::Use(Operand::Copy(Place::Field { base: p, field: 1 })),
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

    let mir = crate::Mir {
        functions: vec![b.finish()],
        layouts,
        ..Default::default()
    };
    let wat = emit_program(&mir, &i);
    assert!(
        wat.contains("i32.const 8"),
        "field 1 sits at byte offset 8:\n{}",
        wat
    );
    assert!(
        wat.contains("f64.load"),
        "a double field loads as f64:\n{}",
        wat
    );
}

#[test]
fn new_allocates_and_initializes_fields() {
    use crate::Rvalue;
    use dream_hir::{FieldLayout, LayoutTable, TypeLayout};
    use dream_types::DefId;
    let mut i = TypeInterner::new();
    let def = DefId(5);
    let int = i.int();
    let sty = i.struct_ty(def, vec![]);

    let mut layouts = LayoutTable::default();
    layouts.insert(
        sty,
        TypeLayout {
            name: "S".into(),
            fields: vec![
                FieldLayout {
                    offset: 0,
                    ty: int,
                    name: "a".into(),
                    is_weak: false,
                    is_unowned: false,
                },
                FieldLayout {
                    offset: 4,
                    ty: int,
                    name: "b".into(),
                    is_weak: false,
                    is_unowned: false,
                },
            ],
            size: 8,
            packed: false,
        },
    );

    // fun make(): S { return S(); }  -- implicit zero-arg default constructor
    let mut b = FunctionBuilder::new("make", sty);
    b.set_def(DefId(9), vec![]);
    let t = b.new_temp(sty);
    b.assign(
        Place::Local(t),
        Rvalue::New {
            def,
            ty: sty,
            ctor: None,
            args: vec![],
        },
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

    let mir = crate::Mir {
        functions: vec![b.finish()],
        layouts,
        ..Default::default()
    };
    let wat = emit_program(&mir, &i);
    assert!(
        wat.contains("i32.const 8"),
        "allocates the struct's data size:\n{}",
        wat
    );
    assert!(
        wat.contains("call $malloc"),
        "constructs via malloc:\n{}",
        wat
    );
    assert!(
        wat.contains("local.set $__obj"),
        "captures the object pointer:\n{}",
        wat
    );
    assert!(
        wat.contains("i32.store"),
        "zero-initializes fields:\n{}",
        wat
    );
}

#[test]
fn strings_get_data_segments_and_addresses() {
    use dream_types::DefId;
    let i = TypeInterner::new();
    let str_ty = i.string();
    let mut b = FunctionBuilder::new("hello", str_ty);
    b.set_def(DefId(1), vec![]);
    b.terminate(Terminator::Return(Some(Operand::Const(Const::Str(
        "hi".into(),
    )))));

    let mir = crate::Mir {
        functions: vec![b.finish()],
        ..Default::default()
    };
    let wat = emit_module(&mir, &i, false);
    // The runtime constants are interned first, then panic messages and protocol strings, so the
    // user's "hi" follows somewhere in the data section. Assert the 8-byte string header layout:
    // unit_len=2, pad=0, then UTF-16 LE 'h','i'.
    assert!(
        wat.contains("(i32.const"),
        "string data pointer const:\n{}",
        wat
    );
    assert!(
        wat.contains("h\\00i\\00"),
        "string data segment for \"hi\" (utf16le payload):\n{}",
        wat
    );
}

#[test]
fn emit_module_assembles_to_valid_wasm() {
    use crate::{Callee, MirGlobal, Rvalue};
    use dream_hir::{FieldLayout, LayoutTable, TypeLayout};
    use dream_types::DefId;
    let mut i = TypeInterner::new();
    let int = i.int();
    let def = DefId(4);
    let sty = i.struct_ty(def, vec![]);

    let mut layouts = LayoutTable::default();
    layouts.insert(
        sty,
        TypeLayout {
            name: "S".into(),
            fields: vec![FieldLayout {
                offset: 0,
                ty: int,
                name: "a".into(),
                is_weak: false,
                is_unowned: false,
            }],
            size: 4,
            packed: false,
        },
    );

    // fun helper(): int { return 7; }  (def 1)
    let mut hb = FunctionBuilder::new("helper", int);
    hb.set_def(DefId(1), vec![]);
    hb.terminate(Terminator::Return(Some(Operand::Const(Const::Int(7)))));

    // fun run(): int {
    //   let o = S(helper());   ; allocation + call + field store
    //   g0 = o.x;              ; global write from a field read
    //   return o.x;
    // }
    let mut rb = FunctionBuilder::new("run", int);
    rb.set_def(DefId(2), vec![]);
    let call_t = rb.new_temp(int);
    rb.assign(
        Place::Local(call_t),
        Rvalue::Call {
            callee: Callee {
                def: DefId(1),
                args: vec![],
                ret: int,
                take_params: vec![],
            },
            args: vec![],
        },
    );
    let obj = rb.new_temp(sty);
    rb.assign(
        Place::Local(obj),
        Rvalue::New {
            def,
            ty: sty,
            ctor: None,
            args: vec![Operand::Copy(Place::Local(call_t))],
        },
    );
    rb.assign(
        Place::Global(crate::Global(0)),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: obj,
            field: 0,
        })),
    );
    rb.terminate(Terminator::Return(Some(Operand::Copy(Place::Field {
        base: obj,
        field: 0,
    }))));

    let mir = crate::Mir {
        functions: vec![hb.finish(), rb.finish()],
        globals: vec![MirGlobal {
            id: crate::Global(0),
            ty: int,
        }],
        layouts,
        ..Default::default()
    };
    let wat = emit_module(&mir, &i, false);
    // The real gate: the emitted module must assemble to valid WebAssembly.
    wat::parse_str(&wat)
        .unwrap_or_else(|e| panic!("emitted module failed to assemble: {}\n{}", e, wat));
}

#[test]
fn emits_arithmetic_function() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("add", i.int());
    let a = b.new_param(i.int(), Some("a".into()));
    let c = b.new_param(i.int(), Some("b".into()));
    let sum = b.new_temp(i.int());
    b.assign(
        Place::Local(sum),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(a)),
            Operand::Copy(Place::Local(c)),
        ),
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(sum)))));
    let func = b.finish();

    let wat = emit_function(&func, &i);
    assert!(wat.contains("(func $add"), "should emit a function header");
    assert!(
        wat.contains("i32.add"),
        "should emit the add instruction:\n{}",
        wat
    );
    assert!(wat.contains("return"));
    // Sync functions use relooper shapes for ordinary CFGs (no `br_table` dispatch).
    assert!(
        !wat.contains("br_table"),
        "simple sync fn should not use br_table dispatch:\n{}",
        wat
    );
}

#[test]
fn shape_emit_while_uses_nested_loop() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("count", i.int());
    let n = b.new_param(i.int(), Some("n".into()));
    let s = b.new_local(i.int(), Some("s".into()));
    b.assign(Place::Local(s), Rvalue::Use(Operand::Const(Const::Int(0))));
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    let cmp = b.new_temp(i.bool());
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(
            BinOp::Lt,
            Operand::Copy(Place::Local(s)),
            Operand::Copy(Place::Local(n)),
        ),
    );
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(cmp)),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    let one = b.new_temp(i.int());
    b.assign(
        Place::Local(one),
        Rvalue::Use(Operand::Const(Const::Int(1))),
    );
    let sum = b.new_temp(i.int());
    b.assign(
        Place::Local(sum),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(s)),
            Operand::Copy(Place::Local(one)),
        ),
    );
    b.assign(
        Place::Local(s),
        Rvalue::Use(Operand::Copy(Place::Local(sum))),
    );
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(s)))));
    let func = b.finish();

    let wat = emit_function(&func, &i);
    assert!(
        wat.contains("loop"),
        "expected relooper loop label:\n{}",
        wat
    );
    assert!(
        !wat.contains("br_table"),
        "sync while should not use br_table dispatch:\n{}",
        wat
    );
}

#[test]
fn shape_emit_if_diamond_uses_nested_if() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("abs_sign", i.int());
    let n = b.new_param(i.int(), Some("n".into()));
    let then_blk = b.new_block();
    let else_blk = b.new_block();
    let join = b.new_block();
    let cmp = b.new_temp(i.bool());
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(
            BinOp::Lt,
            Operand::Copy(Place::Local(n)),
            Operand::Const(Const::Int(0)),
        ),
    );
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(cmp)),
        then_blk,
        else_blk,
    });
    b.switch_to(then_blk);
    b.terminate(Terminator::Goto(join));
    b.switch_to(else_blk);
    b.terminate(Terminator::Goto(join));
    b.switch_to(join);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(n)))));
    let func = b.finish();

    let wat = emit_function(&func, &i);
    assert!(wat.contains("if"), "expected nested if:\n{}", wat);
    assert!(
        !wat.contains("br_table"),
        "sync if should not use br_table dispatch:\n{}",
        wat
    );
}

/// Every `{TAG_*}` placeholder in the object + format runtime must be substituted; interned
/// string pointers are `$__rt_str_*` globals, not `{minus}` / `{STRING_EMPTY}` integers.
#[test]
fn to_string_runtime_has_no_unsubstituted_placeholders() {
    let runtime = to_string_runtime();
    assert!(
        !runtime.contains('{') && !runtime.contains('}'),
        "object/format runtime still contains an unsubstituted placeholder:\n{}",
        runtime
    );
    assert!(
        runtime.contains("global.get $__rt_str_true")
            && runtime.contains("global.get $__rt_str_false")
            && runtime.contains("global.get $__rt_str_minus"),
        "to_string runtime must load interned strings from emitter globals:\n{}",
        runtime
    );
}

/// Debug builds (the default) must actually instrument the allocator under the MIR backend: with
/// `debug` on, `$malloc` bumps the live/total counters; under `--release` (`debug` off) the hot
/// path stays clean. Single-threaded modules also drop the allocator spinlock.
#[test]
fn debug_toggles_allocator_instrumentation() {
    assert!(runtime_prelude(true, false).contains("global.set $live_objects"));
    assert!(!runtime_prelude(false, false).contains("global.set $live_objects"));
    assert!(
        runtime_prelude(false, true).contains("call $__alloc_lock_acquire"),
        "threaded modules must keep the allocator spinlock"
    );
    assert!(
        !runtime_prelude(false, false).contains("call $__alloc_lock_acquire"),
        "single-threaded modules must elide the allocator spinlock"
    );
    assert!(
        runtime_prelude(false, false).contains("global.get $__rt_str_empty"),
        "string runtime must load interned empty from an emitter global"
    );
}

/// Hybrid policy: fused `--release` still opts through handwritten WAT + wasm-opt. Clang output
/// must not be spliced until extract gates pass (see runtime/README.md).
#[test]
fn handwritten_runtime_wat_is_emit_artifact() {
    assert!(include_str!("../../../runtime/panic.wat").contains("call $print_string"));
    assert!(include_str!("../../../runtime/allocator.wat").contains(";;@ALLOC_LOCK_ACQUIRE@"));
}

/// Builds a one-function module carrying a named local and a `DebugLine` marker, so both the
/// debug-info and release emissions can be compared.
fn debug_line_module(i: &TypeInterner) -> crate::Mir {
    use crate::Statement;
    let mut b = FunctionBuilder::new("add", i.int());
    b.set_file(Some("/tmp/thing.dream".to_string()));
    let a = b.new_param(i.int(), Some("a".into()));
    let c = b.new_param(i.int(), Some("b".into()));
    let sum = b.new_local(i.int(), Some("sum".into()));
    // A source-line marker precedes the statement it annotates.
    b.push(Statement::DebugLine(2));
    b.assign(
        Place::Local(sum),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(a)),
            Operand::Copy(Place::Local(c)),
        ),
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(sum)))));
    crate::Mir {
        functions: vec![b.finish()],
        ..Default::default()
    }
}

/// A debug-info build must instrument each `DebugLine` with the `dream_debug` hooks (line/enter/exit),
/// spill named locals into the exported `$__dbg_v*` pool, and emit a source map describing them; and
/// the module must still assemble to valid WebAssembly.
#[test]
fn debug_info_emits_hooks_and_source_map() {
    let i = TypeInterner::new();
    let mir = debug_line_module(&i);
    let (wat, map) = emit_module_with_debug(&mir, &i, false, true, true);

    assert!(
        wat.contains("(import \"dream_debug\" \"line\""),
        "debug-info must import the line hook:\n{}",
        wat
    );
    assert!(wat.contains("(import \"dream_debug\" \"enter\""));
    assert!(wat.contains("(import \"dream_debug\" \"exit\""));
    assert!(
        wat.contains("call $__dbg_enter"),
        "each function announces entry:\n{}",
        wat
    );
    assert!(
        wat.contains("call $__dbg_line") && wat.contains("i32.const 2"),
        "the DebugLine(2) marker lowers to a line hook for file 0, line 2:\n{}",
        wat
    );
    assert!(
        wat.contains("call $__dbg_exit"),
        "each return pops the debugger frame:\n{}",
        wat
    );
    assert!(
        wat.contains("(export \"__dbg_v0\""),
        "named locals are spilled to an exported global pool:\n{}",
        wat
    );

    let map = map.expect("debug-info build must return a source map");
    assert_eq!(map.files, vec!["/tmp/thing.dream".to_string()]);
    assert_eq!(map.functions.len(), 1);
    let func = &map.functions[0];
    let names: Vec<&str> = func.vars.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b", "sum"]);

    wat::parse_str(&wat)
        .unwrap_or_else(|e| panic!("instrumented module failed to assemble: {}\n{}", e, wat));
}

/// A release build (no debug-info) must carry none of the debug hooks and no source map, so it pays
/// zero debugging overhead.
#[test]
fn release_build_has_no_debug_hooks() {
    let i = TypeInterner::new();
    let mir = debug_line_module(&i);
    let (wat, map) = emit_module_with_debug(&mir, &i, false, false, false);
    assert!(map.is_none(), "release build must not produce a source map");
    assert!(!wat.contains("dream_debug"), "no debug imports:\n{}", wat);
    assert!(!wat.contains("__dbg_"), "no debug hooks/pool:\n{}", wat);
}
