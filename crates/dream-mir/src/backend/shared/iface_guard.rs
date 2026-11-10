//! Static guarded devirtualization: an interface slot with at most [`MAX_GUARDED`] implementors
//! dispatches as a tag test into a direct call. Small, receiver-independent integer methods can
//! be evaluated eagerly and selected without an unpredictable branch. Other slots use a switch.
//! The direct arms
//! call through the same function-pointer type the itable uses, so the ABI matches the dynamic
//! path. A guarded slot covers every implementor the itable knows, so a tag outside the arms
//! (including a null receiver) aborts exactly as the itable's missing entry would.

use super::abi_types::c_ident;
use super::cx::Cx;
use super::symbols::func_symbol;
use indexmap::IndexMap;
use indexmap::IndexMap as HashMap;

/// Four is the largest closed set that still beats an itable call once the arms inline.
/// Past that, a mixed receiver is cheaper as an indirect call than as a long test chain.
const MAX_GUARDED: usize = 4;

/// `(runtime tag, C symbol)` arms in tag order.
type Arms = Vec<(i32, String)>;

/// `(iface, slot)` → its guard arms.
pub(crate) type GuardTable = HashMap<(usize, usize), Guard>;

pub(crate) struct Guard {
    pub arms: Arms,
    pub eager: bool,
}

pub(crate) fn build_guards(cx: &Cx<'_>) -> GuardTable {
    let by_def: IndexMap<dream_types::DefId, &crate::MirFunction> =
        cx.mir.functions.iter().map(|f| (f.def, f)).collect();
    let mut arms: IndexMap<(usize, usize), Option<Guard>> = IndexMap::new();
    for imp in &cx.mir.interfaces.impls {
        let tag = cx.interface_tag(imp.class_ty);
        for (iid, definitions) in &imp.entries {
            for (slot, def) in definitions.iter().enumerate() {
                let slot_arms = arms.entry((*iid, slot)).or_insert_with(|| {
                    Some(Guard {
                        arms: Vec::new(),
                        eager: true,
                    })
                });
                let resolved = tag.zip(def.and_then(|def| by_def.get(&def)));
                let (Some(guard), Some((tag, f))) = (slot_arms.as_mut(), resolved) else {
                    *slot_arms = None;
                    continue;
                };
                guard.eager &= eager_safe(f, cx.interner) && !cx.mir.profile.is_debug();
                let list = &mut guard.arms;
                let cname = c_ident(&func_symbol(f));
                match list.iter().find(|(t, _)| *t == tag) {
                    Some((_, prev)) if *prev == cname => {}
                    Some(_) => *slot_arms = None,
                    None => list.push((tag, cname)),
                }
            }
        }
    }
    arms.into_iter()
        .filter_map(|(key, list)| {
            let mut guard = list?;
            // `impls` order is not stable across compiles; tags are.
            guard.arms.sort();
            (!guard.arms.is_empty() && guard.arms.len() <= MAX_GUARDED).then_some((key, guard))
        })
        .collect()
}

// No memory access, ownership, calls, trapping arithmetic or receiver reads may be speculated.
// The small budget bounds extra work when every arm is evaluated instead of only the chosen one.
fn eager_safe(f: &crate::MirFunction, interner: &dream_types::TypeInterner) -> bool {
    use crate::{BinOp, Const, Operand, Place, Rvalue, Statement, Terminator};
    use dream_types::{PrimTy, TyKind};

    let scalar =
        |ty| matches!(interner.kind(ty), TyKind::Prim(p) if !p.is_float() && *p != PrimTy::String);
    let operand = |op: &Operand| match op {
        Operand::Copy(Place::Local(local)) => {
            f.params.first() != Some(local) && scalar(f.local_ty(*local))
        }
        Operand::Const(Const::Int(_) | Const::Long(_) | Const::Bool(_) | Const::Char(_)) => true,
        _ => false,
    };
    if f.is_async
        || f.inline == dream_hir::InlineHint::Never
        || !scalar(f.ret)
        || f.blocks.len() != 1
        || f.blocks[0].stmts.len() > 8
    {
        return false;
    }
    if !matches!(&f.blocks[0].terminator, Terminator::Return(Some(value)) if operand(value)) {
        return false;
    }
    f.blocks[0].stmts.iter().all(|stmt| match stmt {
        Statement::SourceLine(_) | Statement::Nop => true,
        Statement::Assign(Place::Local(dest), rv) if scalar(f.local_ty(*dest)) => match rv {
            Rvalue::Use(value) | Rvalue::Unary(_, value) => operand(value),
            Rvalue::Binary(op, a, b) if !matches!(op, BinOp::Div | BinOp::Rem) => {
                operand(a) && operand(b)
            }
            _ => false,
        },
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::{BinOp, Const, Operand, Place, Rvalue, Statement, Terminator};
    use dream_types::{DefId, TypeInterner};

    #[test]
    fn eager_methods_require_bounded_receiver_independent_nontrapping_arithmetic() {
        let mut i = TypeInterner::new();
        let class = i.struct_ty(DefId::root(1), vec![]);
        for op in [
            BinOp::Add,
            BinOp::Mul,
            BinOp::BitXor,
            BinOp::Div,
            BinOp::Rem,
        ] {
            let mut b = FunctionBuilder::new("apply", i.int());
            b.new_param(class, None);
            let value = b.new_param(i.int(), None);
            let out = b.new_temp(i.int());
            b.assign(
                Place::Local(out),
                Rvalue::Binary(
                    op,
                    Operand::Copy(Place::Local(value)),
                    Operand::Const(Const::Int(3)),
                ),
            );
            b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(out)))));
            let mut f = b.finish();
            assert_eq!(eager_safe(&f, &i), !matches!(op, BinOp::Div | BinOp::Rem));
            f.blocks[0].stmts.push(Statement::Assign(
                Place::Global(crate::Global(0)),
                Rvalue::Use(Operand::Const(Const::Int(1))),
            ));
            assert!(!eager_safe(&f, &i));
        }
        let mut b = FunctionBuilder::new("field", i.int());
        let receiver = b.new_param(class, None);
        let value = b.new_temp(i.int());
        b.assign(
            Place::Local(value),
            Rvalue::Use(Operand::Copy(Place::Field {
                base: receiver,
                field: 0,
            })),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(value)))));
        assert!(!eager_safe(&b.finish(), &i));
    }
}
