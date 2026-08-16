//! Operand evaluation and place load/store.

use super::ModuleEmitter;
use super::names::*;
use dream_hir::scalar_size;
use dream_mir::{Const, MirFunction, Operand, Place};
use dream_types::{TyKind, TypeId};
use std::fmt::Write as _;

impl<'a> ModuleEmitter<'a> {
    pub(crate) fn operand(&mut self, func: &MirFunction, op: &Operand) -> String {
        match op {
            Operand::Copy(p) => self.load_place(func, p),
            Operand::Const(c) => self.const_val(c),
        }
    }

    pub(crate) fn const_val(&mut self, c: &Const) -> String {
        match c {
            Const::Int(v) => format!("{}", *v as i32),
            Const::Long(v) => format!("{}", v),
            Const::Float(v) => llvm_fp_hex(*v),
            Const::F32(v) => llvm_fp_hex(f64::from(*v)),
            Const::Bool(b) => if *b { "1".into() } else { "0".into() },
            Const::Char(ch) => format!("{}", *ch as u32),
            Const::Null => "0".into(),
            Const::Str(s) => self.intern_bytes(s.as_bytes()),
        }
    }

    pub(crate) fn load_place(&mut self, func: &MirFunction, place: &Place) -> String {
        match place {
            Place::Local(l) => {
                let ty = llvm_val_ty(self.interner, func.local_ty(*l));
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = load {}, {}* %l{}", t, ty, ty, l.0);
                t
            }
            Place::Global(g) => {
                let ty = llvm_val_ty(self.interner, self.mir.globals[g.0 as usize].ty);
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = load {}, {}* @g{}", t, ty, ty, g.0);
                t
            }
            Place::Field { base, field } => {
                let bty = func.local_ty(*base);
                let off = self
                    .mir
                    .layouts
                    .get(bty)
                    .and_then(|l| l.fields.get(*field))
                    .map(|f| f.offset)
                    .unwrap_or(0);
                let fty = self
                    .mir
                    .layouts
                    .get(bty)
                    .and_then(|l| l.fields.get(*field))
                    .map(|f| f.ty)
                    .unwrap_or_else(|| self.interner.int());
                let basev = {
                    let ty = llvm_val_ty(self.interner, bty);
                    let t = self.tmp();
                    let _ = writeln!(self.buf, "  {} = load {}, {}* %l{}", t, ty, ty, base.0);
                    t
                };
                let addr = self.tmp();
                let _ = writeln!(self.buf, "  {} = add i32 {}, {}", addr, basev, off);
                self.load_width(fty, &addr)
            }
            Place::Index { base, index, .. } => {
                let bty = func.local_ty(*base);
                let elem = match self.interner.kind(bty) {
                    TyKind::Array(e) => *e,
                    _ => self.interner.int(),
                };
                let (es, _) = scalar_size(self.interner, elem);
                let basev = {
                    let ty = llvm_val_ty(self.interner, bty);
                    let t = self.tmp();
                    let _ = writeln!(self.buf, "  {} = load {}, {}* %l{}", t, ty, ty, base.0);
                    t
                };
                let idx = self.operand(func, index);
                let off = self.tmp();
                let _ = writeln!(self.buf, "  {} = mul i32 {}, {}", off, idx, es);
                let plus = self.tmp();
                let _ = writeln!(self.buf, "  {} = add i32 {}, 4", plus, off);
                let addr = self.tmp();
                let _ = writeln!(self.buf, "  {} = add i32 {}, {}", addr, basev, plus);
                self.load_width(elem, &addr)
            }
            Place::Deref { ptr, elem_ty } => {
                let p = {
                    let t = self.tmp();
                    let _ = writeln!(self.buf, "  {} = load i32, i32* %l{}", t, ptr.0);
                    t
                };
                self.load_width(*elem_ty, &p)
            }
        }
    }

    pub(crate) fn store_place(&mut self, func: &MirFunction, place: &Place, val: &str, retain: bool) {
        match place {
            Place::Local(l) => {
                let dest = func.local_ty(*l);
                let ty = llvm_val_ty(self.interner, dest);
                let _ = writeln!(self.buf, "  store {} {}, {}* %l{}", ty, val, ty, l.0);
            }
            Place::Global(g) => {
                let dest = self.mir.globals[g.0 as usize].ty;
                let ty = llvm_val_ty(self.interner, dest);
                let _ = writeln!(self.buf, "  store {} {}, {}* @g{}", ty, val, ty, g.0);
            }
            Place::Field { base, field } => {
                let bty = func.local_ty(*base);
                let layout = self.mir.layouts.get(bty);
                let field_layout = layout.and_then(|l| l.fields.get(*field));
                let off = field_layout.map(|f| f.offset).unwrap_or(0);
                let fty = field_layout
                    .map(|f| f.ty)
                    .unwrap_or_else(|| self.interner.int());
                let skip_rc = field_layout
                    .map(|f| f.is_weak || f.is_unowned)
                    .unwrap_or(false);
                let basev = {
                    let ty = llvm_val_ty(self.interner, bty);
                    let t = self.tmp();
                    let _ = writeln!(self.buf, "  {} = load {}, {}* %l{}", t, ty, ty, base.0);
                    t
                };
                let addr = self.tmp();
                let _ = writeln!(self.buf, "  {} = add i32 {}, {}", addr, basev, off);
                if retain && !skip_rc && self.interner.is_reference(fty) {
                    let old = self.load_width(fty, &addr);
                    self.retain_if_ref(fty, val);
                    let _ = writeln!(self.buf, "  call void @dream_release(i32 {})", old);
                }
                self.store_width(fty, &addr, val);
            }
            Place::Index { base, index, .. } => {
                let bty = func.local_ty(*base);
                let elem = match self.interner.kind(bty) {
                    TyKind::Array(e) => *e,
                    _ => self.interner.int(),
                };
                let (es, _) = scalar_size(self.interner, elem);
                let basev = {
                    let ty = llvm_val_ty(self.interner, bty);
                    let t = self.tmp();
                    let _ = writeln!(self.buf, "  {} = load {}, {}* %l{}", t, ty, ty, base.0);
                    t
                };
                let idx = self.operand(func, index);
                let off = self.tmp();
                let _ = writeln!(self.buf, "  {} = mul i32 {}, {}", off, idx, es);
                let plus = self.tmp();
                let _ = writeln!(self.buf, "  {} = add i32 {}, 4", plus, off);
                let addr = self.tmp();
                let _ = writeln!(self.buf, "  {} = add i32 {}, {}", addr, basev, plus);
                if retain {
                    self.retain_if_ref(elem, val);
                }
                self.store_width(elem, &addr, val);
            }
            Place::Deref { ptr, elem_ty } => {
                let p = {
                    let t = self.tmp();
                    let _ = writeln!(self.buf, "  {} = load i32, i32* %l{}", t, ptr.0);
                    t
                };
                self.store_width(*elem_ty, &p, val);
            }
        }
    }

    pub(crate) fn retain_if_ref(&mut self, ty: TypeId, val: &str) {
        if self.interner.is_reference(ty) {
            let _ = writeln!(self.buf, "  call void @dream_retain(i32 {})", val);
        }
    }

    pub(crate) fn load_width(&mut self, ty: TypeId, addr: &str) -> String {
        let t = self.tmp();
        let (sz, _) = scalar_size(self.interner, ty);
        match llvm_val_ty(self.interner, ty) {
            "i64" => {
                let _ = writeln!(self.buf, "  {} = call i64 @dream_load_i64(i32 {})", t, addr);
                t
            }
            "float" => {
                let _ = writeln!(self.buf, "  {} = call float @dream_load_f32(i32 {})", t, addr);
                t
            }
            "double" => {
                let _ = writeln!(self.buf, "  {} = call double @dream_load_f64(i32 {})", t, addr);
                t
            }
            _ if sz == 1 => {
                let b = self.tmp();
                let _ = writeln!(self.buf, "  {} = call i8 @dream_load_u8(i32 {})", b, addr);
                let _ = writeln!(self.buf, "  {} = zext i8 {} to i32", t, b);
                t
            }
            _ => {
                let _ = writeln!(self.buf, "  {} = call i32 @dream_load_i32(i32 {})", t, addr);
                t
            }
        }
    }

    pub(crate) fn store_width(&mut self, ty: TypeId, addr: &str, val: &str) {
        let (sz, _) = scalar_size(self.interner, ty);
        match llvm_val_ty(self.interner, ty) {
            "i64" => {
                let _ = writeln!(
                    self.buf,
                    "  call void @dream_store_i64(i32 {}, i64 {})",
                    addr, val
                );
            }
            "float" => {
                let _ = writeln!(
                    self.buf,
                    "  call void @dream_store_f32(i32 {}, float {})",
                    addr, val
                );
            }
            "double" => {
                let _ = writeln!(
                    self.buf,
                    "  call void @dream_store_f64(i32 {}, double {})",
                    addr, val
                );
            }
            _ if sz == 1 => {
                let b = self.tmp();
                let _ = writeln!(self.buf, "  {} = trunc i32 {} to i8", b, val);
                let _ = writeln!(self.buf, "  call void @dream_store_u8(i32 {}, i8 {})", addr, b);
            }
            _ => {
                let _ = writeln!(
                    self.buf,
                    "  call void @dream_store_i32(i32 {}, i32 {})",
                    addr, val
                );
            }
        }
    }
}
