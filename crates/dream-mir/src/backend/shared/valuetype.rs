//! Per-function shadow-frame layout for inline value(`struct`) locals.

use crate::{Local, MirFunction, Operand, Place, Rvalue, Statement};
use dream_hir::LayoutTable;
use dream_types::{TypeId, TypeInterner};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ValueLocalKind {
    Param,
    Borrow,
    Owning,
}

pub(crate) struct ValueFrame {
    slots: HashMap<Local, u32>,
    kinds: HashMap<Local, ValueLocalKind>,
    pub size: u32,
    pub v128_spill: Option<u32>,
    v128_slots: HashMap<Local, u32>,
}

impl ValueFrame {
    pub fn compute(
        func: &MirFunction,
        interner: &TypeInterner,
        layouts: &LayoutTable,
    ) -> ValueFrame {
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
        let mut slots = HashMap::new();
        let mut v128_slots = HashMap::new();
        let mut size = 0u32;
        let mut has_v128 = false;
        for (i, decl) in func.locals.iter().enumerate() {
            if !interner.is_value_type(decl.ty) {
                continue;
            }
            if is_simd_vector(layouts, decl.ty) && i >= param_count {
                has_v128 = true;
                let local = Local(i as u32);
                if !size.is_multiple_of(16) {
                    size += 16 - size % 16;
                }
                v128_slots.insert(local, size);
                size += 16;
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
            if matches!(kind, ValueLocalKind::Owning | ValueLocalKind::Param) {
                let (sz, al) = dream_hir::scalar_size(interner, decl.ty);
                let rem = size % al;
                if rem != 0 {
                    size += al - rem;
                }
                slots.insert(local, size);
                size += sz;
            }
            kinds.insert(local, kind);
        }
        if !size.is_multiple_of(8) {
            size += 8 - size % 8;
        }
        let v128_spill = if has_v128 {
            if !size.is_multiple_of(16) {
                size += 16 - size % 16;
            }
            let off = size;
            size += 16;
            Some(off)
        } else {
            None
        };
        ValueFrame {
            slots,
            kinds,
            size,
            v128_spill,
            v128_slots,
        }
    }

    pub fn kind(&self, l: Local) -> Option<ValueLocalKind> {
        self.kinds.get(&l).copied()
    }

    pub fn v128_slot(&self, l: Local) -> Option<u32> {
        self.v128_slots.get(&l).copied()
    }

    pub fn owning_slots(&self) -> Vec<(Local, u32)> {
        let mut v: Vec<(Local, u32)> = self.slots.iter().map(|(l, o)| (*l, *o)).collect();
        v.sort_by_key(|(_, o)| *o);
        v
    }

    pub fn teardown_slots(&self, func: &MirFunction) -> Vec<(Local, u32)> {
        self.owning_slots()
            .into_iter()
            .filter(|(l, _)| !func.locals[l.0 as usize].manual_drop)
            .collect()
    }
}

pub(crate) fn is_simd_vector(layouts: &LayoutTable, ty: TypeId) -> bool {
    let Some(l) = layouts.get(ty) else {
        return false;
    };
    l.size == 16
        && l.fields.len() == 4
        && l.fields[0].name == "w0"
        && l.fields[1].name == "w1"
        && l.fields[2].name == "w2"
        && l.fields[3].name == "w3"
}

fn is_value_place_copy(rv: &Rvalue) -> bool {
    matches!(
        rv,
        Rvalue::Use(Operand::Copy(
            Place::Local(_) | Place::Field { .. } | Place::Index { .. } | Place::Deref { .. }
        ))
    )
}
