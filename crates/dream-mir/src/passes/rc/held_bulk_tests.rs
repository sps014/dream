use super::*;

#[test]
fn bulk_operations_preserve_disjoint_arrays_and_singleton_slots() {
    for case in 0..8 {
        let mut t = types();
        let array = t.i.array(t.i.string());
        let mut b = FunctionBuilder::new("bulk", t.i.void());
        let base = b.new_param(array, None);
        let other = b.new_local(array, None);
        let x = b.new_local(t.i.string(), None);
        let result = b.new_local(t.i.string(), None);
        let op = |l| Operand::Copy(Place::Local(l));
        let int = |n| Operand::Const(Const::Int(n));
        b.assign(
            Place::Local(other),
            Rvalue::ArrayLit {
                elem_ty: t.i.string(),
                elems: vec![Operand::Const(Const::Null)],
            },
        );
        b.assign(
            Place::Local(x),
            Rvalue::Use(Operand::Copy(Place::index(base, int(0)))),
        );
        b.push(Statement::Retain(op(x)));
        let index = int(if case % 2 == 0 { 1 } else { 0 });
        match case {
            0 | 1 => b.push(Statement::ArrayElemsFill {
                elem_ty: t.i.string(),
                dst: op(base),
                dst_off: index,
                count: int(1),
            }),
            2 | 3 => b.push(Statement::ArraySlotSwap {
                elem_ty: t.i.string(),
                array: op(base),
                left: index,
                other_array: op(base),
                right: int(2),
            }),
            4 | 5 => b.assign(
                Place::Local(result),
                Rvalue::ArraySlotTake {
                    elem_ty: t.i.string(),
                    array: op(base),
                    index,
                },
            ),
            _ => b.assign(
                Place::Local(other),
                Rvalue::ArrayRealloc {
                    elem_ty: t.i.string(),
                    array: op(if case == 6 { base } else { other }),
                    new_len: int(4),
                },
            ),
        }
        b.push(peek_call(&t, x));
        b.push(Statement::Release(op(x)));
        b.terminate(Terminator::Return(None));
        let mut f = b.finish();
        let effects = ModRefTable::compute(
            &Mir {
                functions: vec![peek_fn(&t, false)],
                profile: dream_abi::profile::CompileProfile::Release,
                ..Mir::default()
            },
            &t.i,
        );
        run_function(&mut f, &t.i, &t.layouts, &effects);
        assert_eq!(
            rc_ops(&f, x),
            if matches!(case, 1 | 3 | 5 | 6) { 2 } else { 0 },
            "case={case}"
        );
    }
}
