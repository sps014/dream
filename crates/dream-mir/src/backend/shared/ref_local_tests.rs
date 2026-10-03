use super::{abi_ty, ref_int_locals, AbiTy};
use crate::backend::shared::{cx::Cx, target::Target};
use crate::build::FunctionBuilder;
use crate::{BinOp, Callee, Const, Global, Mir, Operand, Place, Rvalue};
use dream_types::{DefId, TypeInterner};

#[test]
fn synthetic_addresses_do_not_infect_scalar_results() {
    let types = TypeInterner::new();
    let mut b = FunctionBuilder::new("classify", types.void());
    let string = b.new_param(types.string(), None);
    let ptr = b.new_temp(types.int());
    let alias = b.new_temp(types.int());
    let length = b.new_temp(types.int());
    let compared = b.new_temp(types.int());
    let called = b.new_temp(types.int());
    let selected = b.new_temp(types.int());
    let env = b.new_temp(types.int());
    let copied = |l| Operand::Copy(Place::Local(l));
    b.assign(Place::Local(ptr), Rvalue::StrBytes(copied(string)));
    b.assign(Place::Local(alias), Rvalue::Use(copied(ptr)));
    b.assign(Place::Local(length), Rvalue::StrLen(copied(string)));
    b.assign(
        Place::Local(compared),
        Rvalue::Binary(BinOp::Eq, copied(ptr), copied(alias)),
    );
    b.assign(
        Place::Local(called),
        Rvalue::Call {
            callee: Callee {
                def: DefId::root(1),
                args: vec![],
                ret: types.int(),
                take_params: vec![],
            },
            args: vec![copied(ptr)],
        },
    );
    b.assign(
        Place::Local(selected),
        Rvalue::Select {
            cond: copied(ptr),
            then_val: Operand::Const(Const::Int(1)),
            else_val: Operand::Const(Const::Int(2)),
        },
    );
    b.assign(
        Place::Local(env),
        Rvalue::Use(Operand::Copy(Place::Global(Global(0)))),
    );
    let f = b.finish();
    let mir = Mir::default();
    for target in [Target::native(), Target::wasm32()] {
        let cx = Cx::new(&mir, &types, target);
        let refs = ref_int_locals(&cx, &f);
        for l in [ptr, alias, env] {
            assert!(refs[l.0 as usize]);
        }
        for l in [length, compared, called, selected] {
            assert!(!refs[l.0 as usize]);
        }
    }
    assert_eq!(abi_ty(&types, types.js()), AbiTy::I32);
}
