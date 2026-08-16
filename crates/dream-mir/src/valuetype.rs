//! Ownership classification of inline value-struct locals (used by RC-aware inlining).
//! Storage is a guest `i32` address; LLVM alloca/heap implements the bytes.

use crate::{Local, MirFunction, Operand, Place, Rvalue, Statement};
use dream_types::TypeInterner;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ValueLocalKind {
    Param,
    Borrow,
    Owning,
}

pub(crate) struct ValueFrame {
    kinds: HashMap<Local, ValueLocalKind>,
}

impl ValueFrame {
    pub fn compute(func: &MirFunction, interner: &TypeInterner) -> ValueFrame {
        let param_count = func.params.len();
        let mut defs: HashMap<u32, Vec<&Rvalue>> = HashMap::new();
        for block in &func.blocks {
            for stmt in &block.stmts {
                if let Statement::Assign(Place::Local(l), rv) = stmt {
                    defs.entry(l.0).or_default().push(rv);
                }
            }
        }
        let mut kinds = HashMap::new();
        for (i, decl) in func.locals.iter().enumerate() {
            if !interner.is_value_type(decl.ty) {
                continue;
            }
            let local = Local(i as u32);
            let kind = if decl.is_ref || decl.name.as_deref() == Some("this") {
                ValueLocalKind::Borrow
            } else if i < param_count {
                ValueLocalKind::Param
            } else if decl.manual_drop {
                ValueLocalKind::Owning
            } else {
                let alias = decl.name.is_none()
                    && defs
                        .get(&(i as u32))
                        .map(|ds| !ds.is_empty() && ds.iter().all(|rv| is_value_place_copy(rv)))
                        .unwrap_or(false);
                if alias {
                    ValueLocalKind::Borrow
                } else {
                    ValueLocalKind::Owning
                }
            };
            kinds.insert(local, kind);
        }
        ValueFrame { kinds }
    }

    pub fn kind(&self, l: Local) -> Option<ValueLocalKind> {
        self.kinds.get(&l).copied()
    }
}

fn is_value_place_copy(rv: &Rvalue) -> bool {
    matches!(
        rv,
        Rvalue::Use(Operand::Copy(
            Place::Local(_) | Place::Field { .. } | Place::Index { .. } | Place::Deref { .. }
        ))
    )
}
