//! Value storage aliases do not redefine heap-reference locals.

use super::ownership::OwnershipFacts;
use crate::build::FunctionBuilder;
use crate::{Const, Operand, Place, Rvalue, Terminator};
use dream_types::{DefId, TypeInterner};

#[test]
fn value_ref_storage_does_not_alias_heap_reference_local_slots() {
    let mut i = TypeInterner::new();
    let def = DefId::root(740);
    i.mark_value_def(def);
    let record = i.struct_ty(def, vec![]);
    let array = i.array(i.int());
    let mut b = FunctionBuilder::new("provenance", i.void());
    let storage = b.new_local(record, None);
    let a = b.new_local(array, None);
    let other = b.new_local(array, None);
    let loaded = b.new_local(array, None);
    for local in [a, other] {
        b.assign(
            Place::Local(local),
            Rvalue::ArrayNew {
                elem_ty: i.int(),
                len: Operand::Const(Const::Int(1)),
                closure_env: false,
            },
        );
    }
    b.assign(Place::Local(a), Rvalue::Use(Operand::Const(Const::Null)));
    b.assign(
        Place::Local(loaded),
        Rvalue::Use(Operand::Copy(Place::Field {
            base: storage,
            field: 0,
        })),
    );
    b.terminate(Terminator::Return(None));
    let mut f = b.finish();
    f.locals[storage.0 as usize].is_ref = true;
    let facts = OwnershipFacts::compute(&f, &i);
    assert!(!facts.may_alias(0, 2, a, other));
    assert!(facts.may_alias(0, 4, loaded, other));
    assert!(!facts.may_be_nonnull(0, 0, a));
    assert!(facts.may_be_nonnull(0, 2, a));
    assert!(!facts.may_be_nonnull(0, 3, a));
}
