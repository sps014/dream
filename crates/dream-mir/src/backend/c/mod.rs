//! MIR → C99 (`runtime/c/native` or wasm32 via wasi-sdk).

mod ast;
mod builder;
mod c_imports;
mod calls;
mod ctx;
mod emit;
mod js_marshal;
mod module;
mod native_layout;
mod places;
mod print;
mod protocol;
mod release;
mod rvalue;
mod statements;
mod tables;
mod target;
mod terminator;
mod types;

pub use crate::runtime::{
    native_pcre2_include_dir, native_runtime_c_files, native_runtime_include_dir,
    native_runtime_units,
};
pub use module::{emit_c_module, emit_c_module_for};
pub use target::CTarget;

#[cfg(test)]
mod tests {
    use super::emit_c_module;
    use crate::build::FunctionBuilder;
    use crate::{Callee, Const, Mir, Operand, Place, Rvalue, Statement, Terminator};
    use dream_types::TypeInterner;

    #[test]
    fn char_at_is_uint16_payload_load() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("scan", i.int());
        let s = b.new_param(i.string(), Some("s".into()));
        let idx = b.new_param(i.int(), Some("i".into()));
        let t = b.new_local(i.char(), Some("c".into()));
        b.assign(
            Place::Local(t),
            Rvalue::CharAt(
                Operand::Copy(Place::Local(s)),
                Operand::Copy(Place::Local(idx)),
                true,
            ),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(c.contains("dream_char_at_u"), "{}", c);
        assert!(c.contains("dream_rt_native.h"), "{}", c);
        assert!(
            !c.contains("uint8_t t"),
            "char_at must not truncate UTF-16 units through uint8_t:\n{}",
            c
        );
    }

    #[test]
    fn debug_line_emits_hash_line() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        b.set_file(Some("/tmp/prog.dream".into()));
        b.push(Statement::DebugLine(7));
        b.terminate(Terminator::Return(None));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(
            c.contains("#line 7 \"/tmp/prog.dream\""),
            "expected #line from DebugLine:\n{}",
            c
        );
    }

    #[test]
    fn funcbox_env_temp_is_pointer_width() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("call_it", i.void());
        let box_ = b.new_param(i.int(), Some("box".into()));
        let env = b.new_local(i.int(), Some("env".into()));
        b.assign(
            Place::Local(env),
            Rvalue::Call {
                callee: crate::Callee {
                    def: dream_types::DefId(1),
                    args: vec![],
                    ret: i.int(),
                    take_params: vec![],
                },
                args: vec![Operand::Copy(Place::Local(box_))],
            },
        );
        b.terminate(Terminator::Return(None));
        let mir = Mir {
            functions: vec![b.finish()],
            intrinsics: vec![(dream_types::DefId(1), "funcbox_env".into())],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(c.contains("dream_funcbox_env"), "{}", c);
        assert!(
            !c.contains("int32_t t0 = dream_funcbox_env"),
            "{}",
            c
        );
        assert!(
            c.contains("int64_t t0 = dream_funcbox_env")
                || c.contains("dream_ptr t0 = dream_funcbox_env"),
            "{}",
            c
        );
    }

    #[test]
    fn concat_uses_memcpy_helper() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("cat", i.string());
        let a = b.new_param(i.string(), Some("a".into()));
        let c0 = b.new_param(i.string(), Some("b".into()));
        let t = b.new_local(i.string(), Some("r".into()));
        b.assign(
            Place::Local(t),
            Rvalue::Concat(vec![
                Operand::Copy(Place::Local(a)),
                Operand::Copy(Place::Local(c0)),
            ]),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(c.contains("dream_concat_strings"), "{}", c);
    }

    #[test]
    fn concat_int_is_one_alloc() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("cat", i.string());
        let v = b.new_param(i.int(), Some("v".into()));
        let t = b.new_local(i.string(), Some("r".into()));
        b.assign(
            Place::Local(t),
            Rvalue::ConcatInt {
                prefix: Operand::Const(Const::Str("hello".into())),
                value: Operand::Copy(Place::Local(v)),
                suffix: Operand::Const(Const::Str("world".into())),
            },
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(c.contains("dream_concat_str_int_str"), "{}", c);
        assert!(!c.contains("dream_concat_strings("), "{}", c);
    }

    #[test]
    fn substring_raw_maps_to_slice_helper() {
        assert_eq!(
            super::types::runtime_c_name("string_substring_raw"),
            "dream_substring"
        );
        let strings = include_str!("../../runtime/c/native/strings.c");
        assert!(strings.contains("return dream_substring"));
        assert!(!strings.contains("dream_string_alloc(len)"));
        assert!(super::types::native_header_declares("dream_substring_into"));
        assert!(super::types::native_header_declares("dream_concat_strings_into"));
        assert!(super::types::native_header_declares(
            "dream_concat_str_int_str_into"
        ));
    }

    #[test]
    fn substring_rebind_reuses_dest_header() {
        let i = TypeInterner::new();
        let def = dream_types::DefId(1);
        let mut b = FunctionBuilder::new("sub", i.void());
        let s = b.new_param(i.string(), Some("s".into()));
        let dest = b.new_local(i.string(), Some("d".into()));
        let tmp = b.new_local(i.string(), Some("t".into()));
        b.assign(
            Place::Local(tmp),
            Rvalue::Call {
                callee: Callee {
                    def,
                    args: vec![],
                    ret: i.string(),
                    take_params: vec![false, false, false],
                },
                args: vec![
                    Operand::Copy(Place::Local(s)),
                    Operand::Const(Const::Int(1)),
                    Operand::Const(Const::Int(2)),
                ],
            },
        );
        b.push(Statement::Release(Operand::Copy(Place::Local(dest))));
        b.assign(
            Place::Local(dest),
            Rvalue::Use(Operand::Copy(Place::Local(tmp))),
        );
        b.terminate(Terminator::Return(None));
        let mir = Mir {
            functions: vec![b.finish()],
            intrinsics: vec![(def, "string_substring_raw".into())],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(c.contains("dream_substring_into"), "{}", c);
    }

    #[test]
    fn concat_int_rebind_reuses_dest_buffer() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("cat", i.void());
        let v = b.new_param(i.int(), Some("v".into()));
        let dest = b.new_local(i.string(), Some("d".into()));
        b.push(Statement::Release(Operand::Copy(Place::Local(dest))));
        b.assign(
            Place::Local(dest),
            Rvalue::ConcatInt {
                prefix: Operand::Const(Const::Str("hello".into())),
                value: Operand::Copy(Place::Local(v)),
                suffix: Operand::Const(Const::Str("world".into())),
            },
        );
        b.terminate(Terminator::Return(None));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(c.contains("dream_concat_str_int_str_into"), "{}", c);
    }

    #[test]
    fn string_builder_push_maps_to_native_helper() {
        assert_eq!(
            super::types::runtime_c_name("string_builder_push"),
            "dream_sb_push"
        );
        assert!(super::types::native_header_declares("dream_sb_push"));
        assert!(super::types::native_header_declares("dream_sb_grow_bytes"));
        assert!(super::types::native_header_declares("dream_sb_push_units"));
        assert!(include_str!("../../runtime/strings.wat").contains("$string_builder_push"));
    }

    #[test]
    fn field_store_of_call_evals_rhs_once() {
        use crate::Callee;
        use dream_hir::{LayoutTable, TypeLayout};
        use dream_types::{DefKind, TypeCtx};

        let mut ctx = TypeCtx::new();
        let cell_def = ctx.register(DefKind::Struct, "Cell", vec![]);
        let cell_ty = ctx.interner.struct_ty(cell_def, vec![]);
        let layout = TypeLayout::from_fields(
            &ctx.interner,
            "Cell",
            vec![("n".into(), ctx.interner.int(), false, false)],
        );
        let foo = ctx.register(DefKind::Function, "foo", vec![]);
        let fdef = ctx.register(DefKind::Function, "f", vec![]);
        let mut foo_fn = FunctionBuilder::new("foo", ctx.interner.int());
        foo_fn.set_def(foo, vec![]);
        foo_fn.terminate(Terminator::Return(Some(Operand::Const(crate::Const::Int(0)))));
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        b.set_def(fdef, vec![]);
        let obj = b.new_param(cell_ty, Some("o".into()));
        b.assign(
            Place::Field {
                base: obj,
                field: 0,
            },
            Rvalue::Call {
                callee: Callee {
                    def: foo,
                    args: vec![],
                    ret: ctx.interner.int(),
                    take_params: vec![],
                },
                args: vec![],
            },
        );
        b.terminate(Terminator::Return(None));
        let mut layouts = LayoutTable::default();
        layouts.insert(cell_ty, layout);
        let mir = Mir {
            functions: vec![foo_fn.finish(), b.finish()],
            layouts,
            ..Default::default()
        };
        let c = emit_c_module(&mir, &ctx.interner);
        let body = c.split("void f(").nth(1).unwrap_or(&c);
        let calls = body.matches("foo(").count();
        assert_eq!(calls, 1, "call stored into a field must run once:\n{c}");
    }

    #[test]
    fn interned_string_is_not_null() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("hi", i.string());
        let t = b.new_local(i.string(), Some("s".into()));
        b.assign(
            Place::Local(t),
            Rvalue::Use(Operand::Const(Const::Str("hello".into()))),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(!c.contains("0 /* interned str */"), "{}", c);
        assert!(c.contains("TAG_STRING"), "{}", c);
        assert!(c.contains("hello") || c.contains("__ds"), "{}", c);
    }

    #[test]
    fn dense_switch_is_computed_goto() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("disp", i.int());
        let v = b.new_param(i.int(), Some("v".into()));
        let a = b.new_block();
        let cblk = b.new_block();
        let d = b.new_block();
        b.terminate(Terminator::Switch {
            value: Operand::Copy(Place::Local(v)),
            targets: vec![(0, a), (1, cblk)],
            default: d,
        });
        b.switch_to(a);
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
        b.switch_to(cblk);
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(1)))));
        b.switch_to(d);
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(2)))));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(!c.contains("65536"), "{}", c);
        assert!(c.contains("dream_fn_"), "{}", c);
    }

    #[test]
    fn runtime_c_sources_exist() {
        let panic = include_str!("../../runtime/c/native/panic.c");
        let closure = include_str!("../../runtime/c/native/closure.c");
        let heap = include_str!("../../runtime/c/native/heap.c");
        let strings = include_str!("../../runtime/c/native/strings.c");
        let object = include_str!("../../runtime/c/native/object.c");
        let format = include_str!("../../runtime/c/native/format.c");
        assert!(panic.contains("dream_panic"));
        assert!(closure.contains("dream_funcbox_new"));
        assert!(heap.contains("dream_malloc"));
        assert!(strings.contains("dream_string_alloc"));
        assert!(object.contains("dream_box_int"));
        assert!(format.contains("dream_double_to_string"));
    }

    #[test]
    fn future_layout_wide_not_alias_remaining() {
        let n = crate::abi::FutureLayout::native();
        assert_ne!(n.wide, n.remaining);
        assert!(n.awaiting != n.wide);
        let src = include_str!("../../runtime/c/native/async.c");
        assert!(!src.contains("#define F_POLL"), "{}", src);
        assert!(src.contains("F_SLOTS"), "{}", src);
    }

    #[test]
    fn native_rt_uses_memcpy_and_real_pointers() {
        let h = include_str!("../../runtime/c/native/include/dream_rt_native.h");
        assert!(h.contains("memcpy"));
        assert!(h.contains("dream_char_at_u"));
        assert!(h.contains("DREAM_F32_LANES"));
        assert!(
            !h.contains("(uint32_t)addr"),
            "native ABI must not truncate pointers"
        );
        let heap = include_str!("../../runtime/c/native/heap.c");
        assert!(heap.contains("mmap") || heap.contains("VirtualAlloc"));
        assert!(super::types::native_header_declares("print_int"));
        assert!(super::types::native_header_declares("simd_lane_count"));
        assert!(super::types::native_header_declares("dream_malloc"));
        assert!(super::types::native_header_declares("dream_ffi_read_ptr"));
        assert!(super::types::native_header_declares("dream_ffi_read_cstring"));
        assert!(!super::types::native_header_declares("not_a_real_host_fn"));
    }

    #[test]
    fn c_import_emits_named_wrapper() {
        use dream_hir::HImport;
        use dream_types::DefId;
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("main", i.void());
        b.terminate(Terminator::Return(None));
        let mir = Mir {
            functions: vec![b.finish()],
            imports: vec![HImport {
                def: DefId(99),
                name: "sqlite3_open".into(),
                module: "c/sqlite3".into(),
                field: "sqlite3_open".into(),
                params: vec![i.string(), i.long()],
                param_by_ref: vec![false, true],
                ret: Some(i.int()),
                is_async: false,
                c_wide_strings: false,
            }],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(c.contains("dream_c_sqlite3_open"), "{}", c);
        assert!(c.contains("dream_string_to_utf8"), "{}", c);
        assert!(c.contains("sqlite3_open("), "{}", c);
    }

    #[test]
    fn wasm32_runtime_imports_use_host_module() {
        use crate::backend::c::{emit_c_module_for, CTarget};
        use dream_abi::js_abi::HOST_MODULE;
        use dream_hir::HImport;
        use dream_types::DefId;
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("main", i.void());
        b.terminate(Terminator::Return(None));
        let field = dream_abi::runtime_hosts::GPU_TRY_INIT;
        let mir = Mir {
            functions: vec![b.finish()],
            imports: vec![HImport {
                def: DefId(1),
                name: field.into(),
                module: HOST_MODULE.into(),
                field: field.into(),
                params: vec![],
                param_by_ref: vec![],
                ret: Some(i.int()),
                is_async: true,
                c_wide_strings: false,
            }],
            ..Default::default()
        };
        let c = emit_c_module_for(&mir, &i, CTarget::Wasm32);
        assert!(c.contains("dream_rt_wasm32.h"), "{}", c);
        assert!(
            c.contains(&format!("import_module(\"{HOST_MODULE}\")")),
            "{}",
            c
        );
        assert!(c.contains(&format!("import_name(\"{field}\")")), "{}", c);
        assert!(
            c.contains(&format!("export_name(\"{}\")", crate::abi::ENTRY_FN)),
            "{}",
            c
        );
        assert!(!c.contains("dream_process_capture_args"), "{}", c);
    }

    #[test]
    fn wasm32_js_call_fills_tagged_slots() {
        use crate::backend::c::{emit_c_module_for, CTarget};
        use dream_abi::js_abi::{self, HOST_MODULE};
        use dream_hir::HImport;
        use dream_types::DefId;
        let i = TypeInterner::new();
        let js = i.js();
        let mut b = FunctionBuilder::new("main", i.void());
        let tgt = b.new_param(js, Some("t".into()));
        let name = b.new_param(i.string(), Some("n".into()));
        let arg = b.new_param(i.int(), Some("a".into()));
        b.push(Statement::JsCall {
            callee: Callee {
                def: DefId(7),
                args: vec![],
                ret: js,
                take_params: vec![],
            },
            target: Operand::Copy(Place::Local(tgt)),
            via: None,
            method: Some(Operand::Copy(Place::Local(name))),
            args: vec![(Operand::Copy(Place::Local(arg)), i.int())],
        });
        b.terminate(Terminator::Return(None));
        let field = "jsCallV";
        let mir = Mir {
            functions: vec![b.finish()],
            imports: vec![HImport {
                def: DefId(7),
                name: dream_types::method_fn(js_abi::JS_TYPE, "call"),
                module: HOST_MODULE.into(),
                field: field.into(),
                params: vec![js, i.string(), i.int(), i.int()],
                param_by_ref: vec![false, false, false, false],
                ret: Some(js),
                is_async: false,
                c_wide_strings: false,
            }],
            ..Default::default()
        };
        let c = emit_c_module_for(&mir, &i, CTarget::Wasm32);
        assert!(c.contains(field), "{}", c);
        assert!(c.contains(&js_abi::SLOT_SIZE.to_string()), "{}", c);
        assert!(c.contains(&js_abi::tag::INT.to_string()), "{}", c);
        assert!(!c.contains("dream_js_call"), "{}", c);
        let native = emit_c_module(&mir, &i);
        assert!(native.contains("dream_js_call"), "{}", native);
    }

    #[test]
    fn module_needs_threads_on_worker_import() {
        use dream_abi::{js_abi, runtime_hosts};
        use dream_hir::HImport;
        use dream_types::DefId;
        let i = TypeInterner::new();
        let mir = Mir {
            imports: vec![HImport {
                def: DefId(1),
                name: "spawn_host".into(),
                module: js_abi::HOST_MODULE.into(),
                field: runtime_hosts::WORKER_SPAWN.into(),
                params: vec![i.int(), i.long()],
                param_by_ref: vec![false, false],
                ret: Some(i.int()),
                is_async: false,
                c_wide_strings: false,
            }],
            ..Default::default()
        };
        assert!(crate::backend::module_needs_threads(&mir, &i));
        let empty = Mir::default();
        assert!(!crate::backend::module_needs_threads(&empty, &i));
    }

    #[test]
    fn wasm32_exports_worker_invoke() {
        use crate::backend::c::{emit_c_module_for, CTarget};
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("main", i.void());
        b.terminate(Terminator::Return(None));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Default::default()
        };
        let c = emit_c_module_for(&mir, &i, CTarget::Wasm32);
        assert!(
            c.contains(&format!(
                "export_name(\"{}\")",
                crate::abi::EXPORT_WORKER_INVOKE
            )),
            "{}",
            c
        );
        assert!(
            c.contains(&format!(
                "export_name(\"{}\")",
                crate::abi::EXPORT_WORKER_INVOKE_RAW
            )),
            "{}",
            c
        );
        assert!(
            c.contains(&format!(
                "export_name(\"{}\")",
                crate::abi::EXPORT_RUNTIME_INIT
            )),
            "{}",
            c
        );
        assert!(!c.contains("_Thread_local"), "{}", c);
    }
}
