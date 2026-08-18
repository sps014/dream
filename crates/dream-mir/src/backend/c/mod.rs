//! MIR → C99 for the native clang path (`runtime/c/native`).

mod ast;
mod builder;
mod calls;
mod ctx;
mod emit;
mod module;
mod native_layout;
mod places;
mod print;
mod protocol;
mod release;
mod rvalue;
mod statements;
mod tables;
mod terminator;
mod types;

pub use module::{emit_c_module, native_runtime_c_files, native_runtime_include_dir};

#[cfg(test)]
mod tests {
    use super::emit_c_module;
    use crate::build::FunctionBuilder;
    use crate::{Const, Mir, Operand, Place, Rvalue, Terminator};
    use dream_types::TypeInterner;

    #[test]
    fn char_at_is_uint16_payload_load() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("scan", i.int());
        let s = b.new_param(i.string(), Some("s".into()));
        let idx = b.new_param(i.int(), Some("i".into()));
        let t = b.new_local(i.int(), Some("c".into()));
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
        assert!(c.contains("goto *__jt"), "{}", c);
    }

    #[test]
    fn runtime_c_sources_exist() {
        let panic = include_str!("../../runtime/c/panic.c");
        let closure = include_str!("../../runtime/c/closure.c");
        let alloc = include_str!("../../runtime/c/allocator.c");
        let strings = include_str!("../../runtime/c/strings.c");
        let object = include_str!("../../runtime/c/object.c");
        let format = include_str!("../../runtime/c/format.c");
        assert!(panic.contains("dream_panic"));
        assert!(closure.contains("funcbox_new"));
        assert!(alloc.contains("__malloc_locked"));
        assert!(strings.contains("concat_strings"));
        assert!(object.contains("int_to_string"));
        assert!(format.contains("double_to_string"));
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
        let pike = include_str!("../../runtime/c/native/pike.c");
        assert!(pike.contains("goto *"));
        let heap = include_str!("../../runtime/c/native/heap.c");
        assert!(heap.contains("mmap") || heap.contains("VirtualAlloc"));
        assert!(super::types::native_header_declares("print_int"));
        assert!(super::types::native_header_declares("simd_lane_count"));
        assert!(super::types::native_header_declares("dream_malloc"));
        assert!(!super::types::native_header_declares("not_a_real_host_fn"));
    }
}
