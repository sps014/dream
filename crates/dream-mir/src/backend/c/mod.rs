//! MIR → C99 (`runtime/c/native` or wasm32 via wasi-sdk).

mod ast;
mod builder;
mod c_imports;
mod calls;
mod ctx;
mod debugviews;
mod emit;
mod js_marshal;
mod localnames;
mod module;
mod native_layout;
mod places;
mod print;
mod protocol;
mod reach;
mod release;
mod rvalue;
mod shape;
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
    use crate::BinOp;
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
    fn line_groups_reanchor_each_statement() {
        // A dream statement whose C expansion spans several physical lines (call into a temp, then
        // the deferred copy) must not consume its neighbors' line numbers: every statement in a
        // `#line` group re-anchors at the group's line, so breakpoints bind to the statement that
        // actually carries them.
        use crate::backend::c::ast::{CTy, Expr, Func, Param, Stmt};
        let f = Func {
            attr: None,
            export: None,
            static_: false,
            ret: CTy::I32,
            name: "f".into(),
            params: vec![Param {
                ty: CTy::I32,
                name: "x".into(),
            }],
            body: vec![
                Stmt::Line {
                    file: "/tmp/p.dream".into(),
                    line: 15,
                },
                Stmt::decl(CTy::I32, "t0", Some(Expr::local(0))),
                Stmt::assign(Expr::local(1), Expr::local(0)),
                Stmt::Line {
                    file: "/tmp/p.dream".into(),
                    line: 16,
                },
                Stmt::Return(Some(Expr::local(1))),
            ],
        };
        let mut out = String::new();
        super::print::print_func(&mut out, &f);
        let anchored = out.matches("#line 15 \"/tmp/p.dream\"").count();
        assert_eq!(
            anchored, 2,
            "both statements of the line-15 group must re-anchor:\n{out}"
        );
        let after_16 = out.rfind("#line 16").expect("16 directive");
        assert!(
            out[after_16..].contains("return"),
            "return must follow the line-16 directive:\n{}",
            out
        );
        // Functions after a `#line`-carrying one must not inherit its attribution.
        assert!(
            out.contains("#line 1 \"<dream-generated>\""),
            "line group must be closed so later functions don't steal breakpoints:\n{}",
            out
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
        assert!(!c.contains("int32_t t0 = dream_funcbox_env"), "{}", c);
        assert!(
            c.contains("int64_t t0 = dream_funcbox_env")
                || c.contains("dream_ptr t0 = dream_funcbox_env"),
            "{}",
            c
        );
    }

    #[test]
    fn from_bytes_prim_releases_temp_box() {
        let mut i = TypeInterner::new();
        let bytes_ty = i.array(i.byte());
        let mut b = FunctionBuilder::new("parse", i.int());
        let bytes = b.new_param(bytes_ty, Some("bytes".into()));
        let n = b.new_local(i.int(), Some("n".into()));
        b.assign(
            Place::Local(n),
            Rvalue::FromBytes {
                bytes: Operand::Copy(Place::Local(bytes)),
                ty: i.int(),
            },
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(n)))));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(c.contains("dream_from_bytes"), "{}", c);
        assert!(
            c.contains("dream_release"),
            "prim from_bytes must free the scratch box:\n{}",
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
        assert!(c.contains("dream_concat_n"), "{}", c);
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
        assert!(super::types::native_header_declares(
            "dream_concat_strings_into"
        ));
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
        foo_fn.terminate(Terminator::Return(Some(Operand::Const(crate::Const::Int(
            0,
        )))));
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
    fn unique_destroy_cascades_rc_one_fields() {
        use dream_hir::{LayoutTable, TypeLayout};
        use dream_types::{DefKind, TypeCtx};

        let mut ctx = TypeCtx::new();
        let def = ctx.register(DefKind::Struct, "Node", vec![]);
        let ty = ctx.interner.struct_ty(def, vec![]);
        let layout = TypeLayout::from_fields(
            &ctx.interner,
            "Node",
            vec![("left".into(), ty, false, false)],
        );
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let x = b.new_param(ty, Some("x".into()));
        b.push(Statement::ReleaseUnique(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Return(None));
        let mut layouts = LayoutTable::default();
        layouts.insert(ty, layout);
        let mir = Mir {
            functions: vec![b.finish()],
            layouts,
            ..Default::default()
        };
        let c = emit_c_module(&mir, &ctx.interner);
        assert!(
            c.contains("dream_rc_one"),
            "unique destroy should cascade rc==1 fields:\n{}",
            c
        );
        assert!(
            super::types::native_header_declares("dream_rc_one"),
            "dream_rc_one must be in the native header"
        );
        assert!(
            super::types::native_header_declares("dream_region_enter")
                && super::types::native_header_declares("dream_region_leave"),
            "unique-region enter/leave must be in the native header"
        );
    }

    #[test]
    fn if_diamond_emits_structured_if() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("classify", i.int());
        let n = b.new_param(i.int(), Some("n".into()));
        let then_blk = b.new_block();
        let else_blk = b.new_block();
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(n)),
            then_blk,
            else_blk,
        });
        b.switch_to(then_blk);
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(1)))));
        b.switch_to(else_blk);
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(2)))));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(c.contains("if ("), "expected structured if:\n{}", c);
        assert!(c.contains("else"), "expected else arm:\n{}", c);
        assert!(
            !c.contains("goto L"),
            "diamond should not use goto for the branch:\n{}",
            c
        );
        assert!(c.contains("static "), "user fn should be static:\n{}", c);
    }

    #[test]
    fn dense_switch_arms_goto_join() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("disp", i.int());
        let v = b.new_param(i.int(), Some("v".into()));
        let ok = b.new_block();
        let err = b.new_block();
        let join = b.new_block();
        b.terminate(Terminator::Switch {
            value: Operand::Copy(Place::Local(v)),
            targets: vec![(0, ok), (1, err)],
            default: join,
        });
        b.switch_to(ok);
        b.terminate(Terminator::Goto(join));
        b.switch_to(err);
        b.terminate(Terminator::Goto(join));
        b.switch_to(join);
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(c.contains("goto *"), "dense jump table:\n{}", c);
        let ok_pos = c.find("L1:;").or_else(|| c.find("L1:")).unwrap_or(0);
        let err_pos = c.find("L2:;").or_else(|| c.find("L2:")).unwrap_or(0);
        let between = if err_pos > ok_pos {
            &c[ok_pos..err_pos]
        } else {
            ""
        };
        assert!(
            between.contains("goto "),
            "Ok arm must jump to join, not fall into Err:\n{}",
            c
        );
    }

    #[test]
    fn counting_while_inverts_exit() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("count", i.int());
        let n = b.new_param(i.int(), Some("n".into()));
        let acc = b.new_temp(i.int());
        let cmp = b.new_temp(i.int());
        b.assign(
            Place::Local(acc),
            Rvalue::Use(Operand::Const(Const::Int(0))),
        );
        let header = b.new_block();
        let body = b.new_block();
        let done = b.new_block();
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.assign(
            Place::Local(cmp),
            Rvalue::Binary(
                BinOp::Lt,
                Operand::Copy(Place::Local(acc)),
                Operand::Copy(Place::Local(n)),
            ),
        );
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(cmp)),
            then_blk: body,
            else_blk: done,
        });
        b.switch_to(body);
        b.assign(
            Place::Local(acc),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(acc)),
                Operand::Const(Const::Int(1)),
            ),
        );
        b.terminate(Terminator::Goto(header));
        b.switch_to(done);
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(acc)))));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(c.contains("for (;;)"), "loop:\n{}", c);
        assert!(
            c.contains("if (!") || c.contains("if (!("),
            "exit should be inverted if (!cond) break:\n{}",
            c
        );
    }

    #[test]
    fn while_loop_emits_for_forever() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("count", i.int());
        let cond = b.new_block();
        let body = b.new_block();
        let after = b.new_block();
        b.terminate(Terminator::Goto(cond));
        b.switch_to(cond);
        b.terminate(Terminator::If {
            cond: Operand::Const(Const::Bool(true)),
            then_blk: body,
            else_blk: after,
        });
        b.switch_to(body);
        b.terminate(Terminator::Goto(cond));
        b.switch_to(after);
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
        let mir = Mir {
            functions: vec![b.finish()],
            ..Default::default()
        };
        let c = emit_c_module(&mir, &i);
        assert!(c.contains("for (;;)"), "expected C loop:\n{}", c);
        assert!(c.contains("break"), "loop exit should be break:\n{}", c);
        assert!(
            !c.contains("goto L"),
            "header back-edge should not be a goto:\n{}",
            c
        );
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
        assert!(
            c.contains("goto *"),
            "dense switch uses computed goto:\n{}",
            c
        );
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
        assert!(closure.contains("dream_release_object"));
        assert!(heap.contains("dream_malloc"));
        assert!(heap.contains("dream_malloc_shared"));
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
        assert!(super::types::native_header_declares(
            "dream_ffi_read_cstring"
        ));
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
                async_host: false,
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
                async_host: false,
                c_wide_strings: false,
            }],
            ..Default::default()
        };
        let c = emit_c_module_for(&mir, &i, CTarget::Wasm32, false);
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
                async_host: false,
                c_wide_strings: false,
            }],
            ..Default::default()
        };
        let c = emit_c_module_for(&mir, &i, CTarget::Wasm32, false);
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
                async_host: false,
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
        let c = emit_c_module_for(&mir, &i, CTarget::Wasm32, false);
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
