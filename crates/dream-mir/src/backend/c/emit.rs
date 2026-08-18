use super::builder::FuncBuilder;
use super::ctx::Cx;
use super::types::array_elem_ty;
use crate::{Operand, Place};

pub(super) struct Emitter<'a> {
    pub cx: &'a Cx<'a>,
    pub f: &'a crate::MirFunction,
    pub b: &'a mut FuncBuilder,
}

impl<'a> Emitter<'a> {
    pub fn new(cx: &'a Cx<'a>, f: &'a crate::MirFunction, b: &'a mut FuncBuilder) -> Self {
        Self { cx, f, b }
    }

    pub fn operand_ty(&self, o: &Operand) -> dream_types::TypeId {
        match o {
            Operand::Copy(Place::Local(l)) => self.f.local_ty(*l),
            Operand::Copy(Place::Global(g)) => self
                .cx
                .mir
                .globals
                .iter()
                .find(|global| global.id == *g)
                .map(|global| global.ty)
                .unwrap_or_else(|| self.cx.interner.int()),
            Operand::Copy(Place::Field { base, field }) => self
                .cx
                .nstruct(self.f.local_ty(*base))
                .and_then(|layout| layout.fields.get(*field))
                .map(|field| field.ty)
                .unwrap_or_else(|| self.f.local_ty(*base)),
            Operand::Copy(Place::Index { base, .. }) => {
                array_elem_ty(self.cx.interner, self.f.local_ty(*base))
            }
            Operand::Copy(Place::Deref { elem_ty, .. }) => *elem_ty,
            Operand::Const(crate::Const::Str(_)) => self.cx.interner.string(),
            Operand::Const(crate::Const::Long(_)) => self.cx.interner.long(),
            Operand::Const(crate::Const::Float(_)) => self.cx.interner.double(),
            Operand::Const(crate::Const::F32(_)) => self.cx.interner.float(),
            Operand::Const(crate::Const::Bool(_)) => self.cx.interner.bool(),
            Operand::Const(crate::Const::Char(_)) => self.cx.interner.char(),
            Operand::Const(_) => self.cx.interner.int(),
        }
    }
}
