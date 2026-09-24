//! Deterministic textual MIR for `--emit-mir` and tests. Functions, locals, and blocks print in
//! index order; callee/type/field names are resolved through lookups only (never by iterating a
//! hash container), so two compiles of the same module print byte-identical text.

use crate::{
    BasicBlock, Callee, Const, LocalDecl, Mir, MirFunction, Operand, Place, Rvalue, Statement,
    Terminator,
};
use dream_types::{DefId, TyKind, TypeId, TypeInterner};
use indexmap::IndexMap;
use std::fmt::Write;

/// Name resolution for one module snapshot. Owned (layouts and callee names are copied out of the
/// [`Mir`]) so a dump sink can keep it across per-function passes that hold `&mut MirFunction`.
#[derive(Default)]
pub struct MirNames {
    layouts: dream_hir::LayoutTable,
    fns: IndexMap<(DefId, Vec<TypeId>), String>,
    intrinsics: IndexMap<DefId, String>,
}

/// [`MirNames`] paired with the interner for one print call.
pub struct PrettyCx<'a> {
    interner: &'a TypeInterner,
    names: &'a MirNames,
}

impl MirNames {
    pub fn of(mir: &Mir) -> Self {
        let mut fns = IndexMap::new();
        for f in mir.functions.iter().chain(mir.polls.iter()) {
            fns.entry((f.def, f.instance.clone()))
                .or_insert_with(|| f.name.clone());
        }
        let intrinsics = mir
            .intrinsics
            .iter()
            .map(|(d, k)| (*d, k.clone()))
            .collect();
        MirNames {
            layouts: mir.layouts.clone(),
            fns,
            intrinsics,
        }
    }
}

impl<'a> PrettyCx<'a> {
    pub fn new(interner: &'a TypeInterner, names: &'a MirNames) -> Self {
        PrettyCx { interner, names }
    }

    pub fn ty(&self, ty: TypeId) -> String {
        if let Some(l) = self.names.layouts.get(ty) {
            return l.name.clone();
        }
        if let Some(u) = self.names.layouts.union(ty) {
            return u.name.clone();
        }
        match self.interner.kind(ty) {
            TyKind::Prim(p) => p.name().to_string(),
            TyKind::Object => "object".into(),
            TyKind::Void => "void".into(),
            TyKind::Error => "<error>".into(),
            TyKind::Js => "js".into(),
            TyKind::Array(e) => format!("{}[]", self.ty(*e)),
            TyKind::Tuple(es) => format!("({})", self.tys(es)),
            TyKind::Func(ps, r) => format!("fun({}): {}", self.tys(ps), self.ty(*r)),
            TyKind::Struct(d, args)
            | TyKind::Union(d, args)
            | TyKind::Interface(d, args)
                if !args.is_empty() =>
            {
                format!("def{}<{}>", d.0, self.tys(args))
            }
            TyKind::Struct(d, _) | TyKind::Union(d, _) | TyKind::Interface(d, _) | TyKind::Enum(d) => {
                format!("def{}", d.0)
            }
        }
    }

    fn tys(&self, list: &[TypeId]) -> String {
        list.iter()
            .map(|t| self.ty(*t))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn callee(&self, c: &Callee) -> String {
        if let Some(name) = self.names.fns.get(&(c.def, c.args.clone())) {
            return name.clone();
        }
        if let Some(key) = self.names.intrinsics.get(&c.def) {
            return format!("@{key}");
        }
        if c.args.is_empty() {
            format!("def{}", c.def.0)
        } else {
            format!("def{}<{}>", c.def.0, self.tys(&c.args))
        }
    }
}

/// Every function (then every poll body) of `mir` accepted by `keep`, in module order.
pub fn print_module(
    mir: &Mir,
    interner: &TypeInterner,
    keep: impl Fn(&MirFunction) -> bool,
) -> String {
    let names = MirNames::of(mir);
    let cx = PrettyCx::new(interner, &names);
    let mut out = String::new();
    for f in mir.functions.iter().filter(|f| keep(f)) {
        out.push_str(&print_function(&cx, f));
        out.push('\n');
    }
    for p in mir.polls.iter().filter(|f| keep(f)) {
        out.push_str("poll ");
        out.push_str(&print_function(&cx, p));
        out.push('\n');
    }
    out
}

pub fn print_function(cx: &PrettyCx<'_>, func: &MirFunction) -> String {
    let fp = FnPrinter { cx, func };
    let mut out = String::new();
    let params: Vec<String> = func
        .params
        .iter()
        .map(|l| format!("_{}: {}", l.0, cx.ty(func.local_ty(*l))))
        .collect();
    let _ = write!(
        out,
        "fn {}({}) -> {}",
        func.name,
        params.join(", "),
        cx.ty(func.ret)
    );
    if !func.instance.is_empty() {
        let _ = write!(out, " [instance <{}>]", cx.tys(&func.instance));
    }
    if func.is_async {
        out.push_str(" async");
    }
    let _ = writeln!(out, " {{");
    for (i, decl) in func.locals.iter().enumerate() {
        let _ = writeln!(out, "  let _{}: {}{};", i, cx.ty(decl.ty), local_flags(decl));
    }
    for (i, block) in func.blocks.iter().enumerate() {
        let entry = if i == func.entry.0 as usize {
            " (entry)"
        } else {
            ""
        };
        let _ = writeln!(out, "  bb{i}:{entry}");
        fp.block(&mut out, block);
    }
    let _ = writeln!(out, "}}");
    out
}

fn local_flags(decl: &LocalDecl) -> String {
    let mut s = String::new();
    if let Some(name) = &decl.name {
        let _ = write!(s, " // {name}");
    }
    for (on, tag) in [
        (decl.is_take, "take"),
        (decl.is_ref, "ref"),
        (decl.is_cursor, "cursor"),
        (decl.manual_drop, "manual_drop"),
    ] {
        if on {
            if s.is_empty() {
                s.push_str(" //");
            }
            let _ = write!(s, " [{tag}]");
        }
    }
    s
}

struct FnPrinter<'a> {
    cx: &'a PrettyCx<'a>,
    func: &'a MirFunction,
}

impl FnPrinter<'_> {
    fn block(&self, out: &mut String, block: &BasicBlock) {
        for s in &block.stmts {
            let _ = writeln!(out, "    {}", self.stmt(s));
        }
        let _ = writeln!(out, "    {}", self.terminator(&block.terminator));
    }

    fn stmt(&self, s: &Statement) -> String {
        match s {
            Statement::Assign(p, r) => format!("{} = {}", self.place(p), self.rvalue(r)),
            Statement::Retain(o) => format!("retain {}", self.operand(o)),
            Statement::Release(o) => format!("release {}", self.operand(o)),
            Statement::ReleaseUnique(o) => format!("release_unique {}", self.operand(o)),
            Statement::Panic(o) => format!("panic {}", self.operand(o)),
            Statement::Call { callee, args } => {
                format!("call {}({})", self.cx.callee(callee), self.ops(args))
            }
            Statement::JsCall {
                callee,
                target,
                via,
                method,
                args,
            } => self.js_call(callee, target, via, method, args),
            Statement::InterfaceCall {
                receiver,
                iface_id,
                method_slot,
                args,
                ..
            } => format!(
                "iface_call I{}#{} {}({})",
                iface_id,
                method_slot,
                self.operand(receiver),
                self.ops(args)
            ),
            Statement::IndirectCall { target, args, .. } => {
                format!("indirect_call {}({})", self.operand(target), self.ops(args))
            }
            Statement::Print { arg, newline, .. } => {
                let f = if *newline { "println" } else { "print" };
                format!("{}({})", f, self.operand(arg))
            }
            Statement::Nop => "nop".to_string(),
            Statement::DebugLine(line) => format!("dbg_line {line}"),
            Statement::SourceLine(line) => format!("src_line {line}"),
            Statement::ForceFree(o) => format!("force_free {}", self.operand(o)),
            Statement::ArrayElemsCopy {
                elem_ty,
                dst,
                dst_off,
                src,
                src_off,
                count,
            } => format!(
                "array_elems_copy::<{}>({}, {}, {}, {}, {})",
                self.cx.ty(*elem_ty),
                self.operand(dst),
                self.operand(dst_off),
                self.operand(src),
                self.operand(src_off),
                self.operand(count)
            ),
            Statement::ArrayElemsFill {
                elem_ty,
                dst,
                dst_off,
                count,
            } => format!(
                "array_elems_fill::<{}>({}, {}, {})",
                self.cx.ty(*elem_ty),
                self.operand(dst),
                self.operand(dst_off),
                self.operand(count)
            ),
            Statement::LockAcquire(o) => format!("lock_acquire {}", self.operand(o)),
            Statement::LockRelease(o) => format!("lock_release {}", self.operand(o)),
            Statement::DeferEnter => "defer_enter".into(),
            Statement::DeferLeave(o) => format!("defer_leave {}", self.operand(o)),
            Statement::RegionEnter => "region_enter".into(),
            Statement::RegionLeave => "region_leave".into(),
            Statement::SimdV128 {
                lane,
                op,
                dest,
                lhs,
                rhs,
                index,
                splat_rhs,
                ptr_addr,
            } => format!(
                "simd_v128<{:?}, {:?}{}>({}, {}, {}, {}{})",
                lane,
                op,
                if *ptr_addr { ", ptr" } else { "" },
                self.operand(dest),
                self.operand(lhs),
                self.operand(rhs),
                self.operand(index),
                splat_rhs
                    .as_ref()
                    .map(|s| format!(", splat={}", self.operand(s)))
                    .unwrap_or_default()
            ),
            Statement::ValueDrop(l) => format!("value_drop _{}", l.0),
            Statement::ValueRetain(l) => format!("value_retain _{}", l.0),
            Statement::ValueKill(l) => format!("value_kill _{}", l.0),
        }
    }

    fn terminator(&self, t: &Terminator) -> String {
        match t {
            Terminator::Goto(b) => format!("goto bb{}", b.0),
            Terminator::If {
                cond,
                then_blk,
                else_blk,
            } => format!(
                "if {} -> bb{} else bb{}",
                self.operand(cond),
                then_blk.0,
                else_blk.0
            ),
            Terminator::Switch {
                value,
                targets,
                default,
            } => {
                let arms: Vec<String> = targets
                    .iter()
                    .map(|(v, b)| format!("{} -> bb{}", v, b.0))
                    .collect();
                format!(
                    "switch {} [{}] else bb{}",
                    self.operand(value),
                    arms.join(", "),
                    default.0
                )
            }
            Terminator::Return(Some(o)) => format!("return {}", self.operand(o)),
            Terminator::Return(None) => "return".to_string(),
            Terminator::AsyncComplete(Some(o)) => format!("async_complete {}", self.operand(o)),
            Terminator::AsyncComplete(None) => "async_complete".to_string(),
            Terminator::Await {
                future,
                dest,
                resume,
            } => format!(
                "await {}{} -> bb{}",
                self.operand(future),
                dest.map(|d| format!(" into _{}", d.0)).unwrap_or_default(),
                resume.0
            ),
            Terminator::TailCall { callee, args } => {
                format!("tail_call {}({})", self.cx.callee(callee), self.ops(args))
            }
            Terminator::Unreachable => "unreachable".to_string(),
        }
    }

    fn rvalue(&self, r: &Rvalue) -> String {
        match r {
            Rvalue::Use(o) => self.operand(o),
            Rvalue::Move { src, cast } => match cast {
                Some((from, to)) => format!(
                    "move _{} as {} (from {})",
                    src.0,
                    self.cx.ty(*to),
                    self.cx.ty(*from)
                ),
                None => format!("move _{}", src.0),
            },
            Rvalue::Select {
                cond,
                then_val,
                else_val,
            } => format!(
                "select({}, {}, {})",
                self.operand(cond),
                self.operand(then_val),
                self.operand(else_val)
            ),
            Rvalue::Binary(op, a, b) => {
                format!("{:?}({}, {})", op, self.operand(a), self.operand(b))
            }
            Rvalue::Unary(op, a) => format!("{:?}({})", op, self.operand(a)),
            Rvalue::CheckedBinary(op, a, b) => {
                format!("Checked{:?}({}, {})", op, self.operand(a), self.operand(b))
            }
            Rvalue::CheckedNeg(a) => format!("CheckedNeg({})", self.operand(a)),
            Rvalue::Call { callee, args } => {
                format!("call {}({})", self.cx.callee(callee), self.ops(args))
            }
            Rvalue::IndirectCall { target, args, .. } => {
                format!("call_indirect {}({})", self.operand(target), self.ops(args))
            }
            Rvalue::InterfaceCall {
                receiver,
                iface_id,
                method_slot,
                args,
                ..
            } => format!(
                "iface_call I{}#{} {}({})",
                iface_id,
                method_slot,
                self.operand(receiver),
                self.ops(args)
            ),
            Rvalue::New { ty, ctor, args, .. } => match ctor {
                Some(c) => format!(
                    "new {} via {}({})",
                    self.cx.ty(*ty),
                    self.cx.callee(&Callee {
                        def: c.def,
                        args: Vec::new(),
                        ret: *ty,
                        take_params: Vec::new(),
                    }),
                    self.ops(args)
                ),
                None => format!("new {}({})", self.cx.ty(*ty), self.ops(args)),
            },
            Rvalue::UnionNew {
                ty, variant, args, ..
            } => format!(
                "{}::{}({})",
                self.cx.ty(*ty),
                self.variant_name(*ty, *variant),
                self.ops(args)
            ),
            Rvalue::ArrayLit { elem_ty, elems } => {
                format!("[{}; {}]", self.ops(elems), self.cx.ty(*elem_ty))
            }
            Rvalue::Tuple { elems, .. } => format!("({})", self.ops(elems)),
            Rvalue::ArrayLen(o) => format!("len({})", self.operand(o)),
            Rvalue::StrLen(o) => format!("str_scalar_len({})", self.operand(o)),
            Rvalue::StrByteSize(o) => format!("str_byte_size({})", self.operand(o)),
            Rvalue::CharAt(s, i, unchecked) => {
                let op = if *unchecked { "char_at_u" } else { "char_at" };
                format!("{}({}, {})", op, self.operand(s), self.operand(i))
            }
            Rvalue::ByteAt(s, i, unchecked) => {
                let op = if *unchecked { "byte_at_u" } else { "byte_at" };
                format!("{}({}, {})", op, self.operand(s), self.operand(i))
            }
            Rvalue::ArrayNew { elem_ty, len } => {
                format!("array_new::<{}>({})", self.cx.ty(*elem_ty), self.operand(len))
            }
            Rvalue::ToBytes { value, ty } => {
                format!("to_bytes::<{}>({})", self.cx.ty(*ty), self.operand(value))
            }
            Rvalue::FromBytes { bytes, ty } => {
                format!("from_bytes::<{}>({})", self.cx.ty(*ty), self.operand(bytes))
            }
            Rvalue::ArrayRealloc {
                elem_ty,
                array,
                new_len,
            } => format!(
                "array_realloc::<{}>({}, {})",
                self.cx.ty(*elem_ty),
                self.operand(array),
                self.operand(new_len)
            ),
            Rvalue::HashCode(o) => format!("hash_code({})", self.operand(o)),
            Rvalue::ToString(o) => format!("to_string({})", self.operand(o)),
            Rvalue::Concat(parts) => format!("concat({})", self.ops(parts)),
            Rvalue::ConcatInt {
                prefix,
                value,
                suffix,
            } => format!(
                "concat_int({}, {}, {})",
                self.operand(prefix),
                self.operand(value),
                self.operand(suffix)
            ),
            Rvalue::EnumName { value, .. } => format!("enum_name({})", self.operand(value)),
            Rvalue::Cast(o, from, to) => format!(
                "{} as {} (from {})",
                self.operand(o),
                self.cx.ty(*to),
                self.cx.ty(*from)
            ),
            Rvalue::Discriminant { base, ty } => {
                format!("discriminant<{}>({})", self.cx.ty(*ty), self.operand(base))
            }
            Rvalue::IsType(o, ty) => format!("{} is {}", self.operand(o), self.cx.ty(*ty)),
            Rvalue::TypeName(o) => format!("typeof({})", self.operand(o)),
            Rvalue::UnionField {
                base,
                ty,
                variant,
                field,
            } => {
                let fname = self
                    .cx
                    .names
                    .layouts
                    .union(*ty)
                    .and_then(|u| u.variants.get(*variant))
                    .and_then(|v| v.fields.get(*field))
                    .map(|f| f.name.clone())
                    .unwrap_or_else(|| field.to_string());
                format!(
                    "{} as {}.{}",
                    self.operand(base),
                    self.variant_name(*ty, *variant),
                    fname
                )
            }
            Rvalue::FuncRef(callee) => format!("funcref {}", self.cx.callee(callee)),
            Rvalue::JsCall {
                callee,
                target,
                via,
                method,
                args,
            } => self.js_call(callee, target, via, method, args),
        }
    }

    fn js_call(
        &self,
        callee: &Callee,
        target: &Operand,
        via: &Option<Operand>,
        method: &Option<Operand>,
        args: &[(Operand, TypeId)],
    ) -> String {
        let v = via
            .as_ref()
            .map(|p| format!("{}.", self.operand(p)))
            .unwrap_or_default();
        let m = method
            .as_ref()
            .map(|o| self.operand(o))
            .unwrap_or_else(|| "*".to_string());
        let a = args
            .iter()
            .map(|(o, _)| self.operand(o))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "js_call {} {}[{}{}]({})",
            self.cx.callee(callee),
            self.operand(target),
            v,
            m,
            a
        )
    }

    fn variant_name(&self, ty: TypeId, variant: usize) -> String {
        self.cx
            .names
            .layouts
            .union(ty)
            .and_then(|u| u.variants.get(variant))
            .map(|v| v.name.clone())
            .unwrap_or_else(|| format!("#{variant}"))
    }

    fn operand(&self, o: &Operand) -> String {
        match o {
            Operand::Copy(p) => self.place(p),
            Operand::Const(c) => constant(c),
        }
    }

    fn ops(&self, list: &[Operand]) -> String {
        list.iter()
            .map(|o| self.operand(o))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn place(&self, p: &Place) -> String {
        match p {
            Place::Local(l) => format!("_{}", l.0),
            Place::Global(g) => format!("@g{}", g.0),
            Place::Field { base, field } => {
                let name = self
                    .func
                    .locals
                    .get(base.0 as usize)
                    .and_then(|d| self.cx.names.layouts.get(d.ty))
                    .and_then(|l| l.fields.get(*field))
                    .map(|f| f.name.clone())
                    .unwrap_or_else(|| field.to_string());
                format!("_{}.{}", base.0, name)
            }
            Place::Index {
                base,
                index,
                unchecked,
            } => {
                let u = if *unchecked { " unchecked" } else { "" };
                format!("_{}[{}{}]", base.0, self.operand(index), u)
            }
            Place::Deref { ptr, elem_ty } => format!("*(_{} as {}*)", ptr.0, self.cx.ty(*elem_ty)),
        }
    }
}

fn constant(c: &Const) -> String {
    match c {
        Const::Int(v) => v.to_string(),
        Const::Long(v) => format!("{v}L"),
        Const::Float(v) => format!("{v:?}"),
        Const::F32(v) => format!("{v:?}f"),
        Const::Bool(v) => v.to_string(),
        Const::Char(v) => format!("{v:?}"),
        Const::Str(s) => format!("{s:?}"),
        Const::Null => "null".to_string(),
    }
}
