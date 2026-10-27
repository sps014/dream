//! Element writes share the same location model for direct statements and call summaries.

use crate::analysis::ownership::constant_index;
use crate::{Local, MirFunction, Operand, Place, Rvalue, Statement};
use dream_types::{TyKind, TypeId, TypeInterner};

pub(crate) struct ElementWrite {
    pub(crate) element: TypeId,
    pub(crate) base: Local,
    pub(crate) index: Option<i64>,
}

pub(crate) fn element_writes(
    stmt: &Statement,
    f: &MirFunction,
    i: &TypeInterner,
) -> Option<Vec<ElementWrite>> {
    let mut writes = vec![];
    let mut record = |element, array: &Operand, index| {
        let Operand::Copy(Place::Local(base)) = array else {
            return None;
        };
        writes.push(ElementWrite {
            element,
            base: *base,
            index,
        });
        Some(())
    };
    match stmt {
        Statement::Assign(
            _,
            Rvalue::ArraySlotTake {
                elem_ty,
                array,
                index,
            },
        ) => record(*elem_ty, array, constant_index(index))?,
        Statement::Assign(_, Rvalue::ArrayRealloc { elem_ty, array, .. }) => {
            record(*elem_ty, array, None)?
        }
        Statement::ArrayElemsCopy {
            elem_ty,
            dst,
            dst_off,
            count,
            ..
        }
        | Statement::ArrayElemsFill {
            elem_ty,
            dst,
            dst_off,
            count,
        } => {
            let index = (constant_index(count) == Some(1))
                .then(|| constant_index(dst_off))
                .flatten();
            record(*elem_ty, dst, index)?;
        }
        Statement::ArraySlotSwap {
            elem_ty,
            array,
            left,
            other_array,
            right,
        } => {
            record(*elem_ty, array, constant_index(left))?;
            record(*elem_ty, other_array, constant_index(right))?;
        }
        Statement::Assign(Place::Index { .. }, _) => {}
        _ => return None,
    }
    if let Statement::Assign(Place::Index { base, index, .. }, _) = stmt {
        let TyKind::Array(element) = i.kind(f.local_ty(*base)) else {
            return None;
        };
        writes.push(ElementWrite {
            element: *element,
            base: *base,
            index: constant_index(index),
        });
    }
    Some(writes)
}
