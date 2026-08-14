//! Function-exit `$dream_drop` of owning heap locals and `move` parameters.
//!
//! A local is an alias (not dropped) when it is returned, loaded from a field/index, stored into
//! live heap, copied from a borrow parameter, or passed to a callee parameter that itself escapes.
//! Instance `this` is not treated as an escaping *argument*: the caller still owns the receiver
//! (`xs.get` must not keep `xs` alive). Borrow parameters are never dropped.

use super::{MirFunction, MirPass};
use crate::{Callee, Mir, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, TypeId, TypeInterner};
use indexmap::IndexMap;

pub struct InsertDrops;

pub(crate) type ParamEscapes = IndexMap<(DefId, Vec<TypeId>), Vec<bool>>;
pub(crate) type RetAlias = IndexMap<(DefId, Vec<TypeId>), bool>;

pub(crate) struct EscapeInfo {
    pub params: ParamEscapes,
    pub ret_alias: RetAlias,
}

impl EscapeInfo {
    fn empty() -> Self {
        Self {
            params: IndexMap::new(),
            ret_alias: IndexMap::new(),
        }
    }
}

impl MirPass for InsertDrops {
    fn name(&self) -> &'static str {
        "insert-drops"
    }

    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool {
        self.run_with(func, interner, &EscapeInfo::empty())
    }
}

impl InsertDrops {
    pub(crate) fn run_with(
        &self,
        func: &mut MirFunction,
        interner: &TypeInterner,
        info: &EscapeInfo,
    ) -> bool {
        let drop_locals: Vec<bool> = func
            .locals
            .iter()
            .map(|d| interner.needs_drop(d.ty))
            .collect();
        if !drop_locals.iter().any(|d| *d) {
            return false;
        }
        let escaped = drop_aliases(func, drop_locals.len(), info, interner);
        let copied = locals_copied_as_ptr(func, interner, drop_locals.len());
        let mut changed = false;
        let nblocks = func.blocks.len();
        for bi in 0..nblocks {
            let stmts = std::mem::take(&mut func.blocks[bi].stmts);
            let term = func.blocks[bi].terminator.clone();
            let mut out = Vec::with_capacity(stmts.len() + 8);
            for stmt in stmts {
                // Array slots are overwritten in loops (`Buffer.alloc`); reclaiming the previous
                // block keeps the freelist hot. Eager drop of strings/class temps is slower than
                // bump-and-forget and is unsafe if IPA missed a stored alias.
                if let Statement::Assign(Place::Local(d), rv) = &stmt {
                    let i = d.0 as usize;
                    if i < drop_locals.len()
                        && drop_locals[i]
                        && interner.unwrap_array(func.locals[i].ty).is_some()
                        && !is_borrow_local(func, i)
                        && !escaped[i]
                        && !copied[i]
                        && !rvalue_uses_local(rv, *d)
                    {
                        out.push(Statement::ForceFree(Operand::Copy(Place::Local(*d))));
                        changed = true;
                    }
                }
                out.push(stmt);
            }
            let at_exit = matches!(
                term,
                Terminator::Return(_) | Terminator::AsyncComplete(_) | Terminator::TailCall { .. }
            );
            if at_exit {
                for (i, needs) in drop_locals.iter().enumerate() {
                    if !*needs || is_borrow_local(func, i) || escaped[i] || exit_keeps(&term, i as u32)
                    {
                        continue;
                    }
                    out.push(Statement::ForceFree(Operand::Copy(Place::Local(
                        crate::Local(i as u32),
                    ))));
                    changed = true;
                }
            }
            func.blocks[bi].stmts = out;
        }
        changed
    }
}

struct CopiedMark<'a> {
    copied: &'a mut [bool],
    func: &'a MirFunction,
    interner: &'a TypeInterner,
}

impl CopiedMark<'_> {
    fn mark(&mut self, l: crate::Local) {
        if let Some(slot) = self.copied.get_mut(l.0 as usize) {
            *slot = true;
        }
    }

    fn operand(&mut self, o: &Operand) {
        match o {
            Operand::Copy(Place::Local(l)) => self.mark(*l),
            Operand::Copy(Place::Field { base, .. }) => self.mark(*base),
            Operand::Copy(Place::Deref { ptr, elem_ty }) => {
                if self.interner.needs_drop(*elem_ty) {
                    self.mark(*ptr);
                }
            }
            Operand::Copy(Place::Index { base, index, .. }) => {
                if let Some(decl) = self.func.locals.get(base.0 as usize) {
                    if self
                        .interner
                        .unwrap_array(decl.ty)
                        .is_some_and(|e| self.interner.needs_drop(e))
                    {
                        self.mark(*base);
                    }
                }
                self.operand(index);
            }
            Operand::Copy(Place::Global(_)) | Operand::Const(_) => {}
        }
    }

    fn rvalue(&mut self, rv: &Rvalue) {
        match rv {
            Rvalue::Use(o)
            | Rvalue::Cast(o, _, _)
            | Rvalue::UnionField { base: o, .. } => self.operand(o),
            Rvalue::ArrayRealloc {
                array: a, new_len: b, ..
            } => {
                self.operand(a);
                self.operand(b);
            }
            Rvalue::Unary(_, _)
            | Rvalue::StrLen(_)
            | Rvalue::StrByteSize(_)
            | Rvalue::HashCode(_)
            | Rvalue::ToString(_)
            | Rvalue::ArrayLen(_)
            | Rvalue::Discriminant(_)
            | Rvalue::IsType(_, _)
            | Rvalue::EnumName { .. }
            | Rvalue::ToBytes { .. }
            | Rvalue::FromBytes { .. }
            | Rvalue::ArrayNew { .. }
            | Rvalue::Binary(_, _, _)
            | Rvalue::CharAt(_, _)
            | Rvalue::ByteAt(_, _)
            | Rvalue::Concat(_, _) => {}
            Rvalue::Select {
                cond,
                then_val,
                else_val,
            } => {
                self.operand(cond);
                self.operand(then_val);
                self.operand(else_val);
            }
            Rvalue::Call { args, .. }
            | Rvalue::New { args, .. }
            | Rvalue::UnionNew { args, .. }
            | Rvalue::Tuple { elems: args, .. }
            | Rvalue::ArrayLit { elems: args, .. } => {
                for a in args {
                    self.operand(a);
                }
            }
            Rvalue::IndirectCall { target, args, .. } => {
                self.operand(target);
                for a in args {
                    self.operand(a);
                }
            }
            Rvalue::InterfaceCall {
                receiver, args, ..
            } => {
                self.operand(receiver);
                for a in args {
                    self.operand(a);
                }
            }
            Rvalue::JsCall {
                target,
                via,
                method,
                args,
                ..
            } => {
                self.operand(target);
                if let Some(v) = via {
                    self.operand(v);
                }
                if let Some(m) = method {
                    self.operand(m);
                }
                for (a, _) in args {
                    self.operand(a);
                }
            }
            Rvalue::FuncRef(_) => {}
        }
    }
}

fn locals_copied_as_ptr(func: &MirFunction, interner: &TypeInterner, n: usize) -> Vec<bool> {
    let mut copied = vec![false; n];
    let mut mark = CopiedMark {
        copied: &mut copied,
        func,
        interner,
    };
    for block in &func.blocks {
        for stmt in &block.stmts {
            match stmt {
                Statement::Assign(_, rv) | Statement::AssignNoDrop(_, rv) => mark.rvalue(rv),
                Statement::Panic(o)
                | Statement::ForceFree(o)
                | Statement::LockAcquire(o)
                | Statement::LockRelease(o)
                | Statement::ArenaEnter(o)
                | Statement::Print { arg: o, .. } => mark.operand(o),
                Statement::Call { args, .. } => {
                    for a in args {
                        mark.operand(a);
                    }
                }
                Statement::IndirectCall { target, args, .. } => {
                    mark.operand(target);
                    for a in args {
                        mark.operand(a);
                    }
                }
                Statement::InterfaceCall {
                    receiver, args, ..
                } => {
                    mark.operand(receiver);
                    for a in args {
                        mark.operand(a);
                    }
                }
                Statement::JsCall {
                    target,
                    via,
                    method,
                    args,
                    ..
                } => {
                    mark.operand(target);
                    if let Some(v) = via {
                        mark.operand(v);
                    }
                    if let Some(m) = method {
                        mark.operand(m);
                    }
                    for (a, _) in args {
                        mark.operand(a);
                    }
                }
                Statement::ArrayElemsCopy {
                    dst,
                    dst_off,
                    src,
                    src_off,
                    count,
                    ..
                } => {
                    mark.operand(dst);
                    mark.operand(dst_off);
                    mark.operand(src);
                    mark.operand(src_off);
                    mark.operand(count);
                }
                Statement::SimdF32x4 {
                    dest,
                    lhs,
                    rhs,
                    index,
                    ..
                } => {
                    mark.operand(dest);
                    mark.operand(lhs);
                    mark.operand(rhs);
                    mark.operand(index);
                }
                Statement::Nop
                | Statement::DebugLine(_)
                | Statement::SourceLine(_)
                | Statement::ArenaExit
                | Statement::ValueDrop(_) => {}
            }
        }
        match &block.terminator {
            Terminator::If { cond, .. } => mark.operand(cond),
            Terminator::Switch { value, .. } => mark.operand(value),
            Terminator::Return(o) | Terminator::AsyncComplete(o) => {
                if let Some(op) = o {
                    mark.operand(op);
                }
            }
            Terminator::Await { future, .. } => mark.operand(future),
            Terminator::TailCall { args, .. } => {
                for a in args {
                    mark.operand(a);
                }
            }
            Terminator::Goto(_) | Terminator::Unreachable => {}
        }
    }
    copied
}

fn operand_uses_local(o: &Operand, local: crate::Local) -> bool {
    match o {
        Operand::Copy(Place::Local(l)) => *l == local,
        Operand::Copy(Place::Field { base, .. }) => *base == local,
        Operand::Copy(Place::Index { base, index, .. }) => {
            *base == local || operand_uses_local(index, local)
        }
        Operand::Copy(Place::Deref { ptr, .. }) => *ptr == local,
        Operand::Copy(Place::Global(_)) | Operand::Const(_) => false,
    }
}

fn rvalue_uses_local(rv: &Rvalue, local: crate::Local) -> bool {
    match rv {
        Rvalue::Use(o)
        | Rvalue::Unary(_, o)
        | Rvalue::StrLen(o)
        | Rvalue::StrByteSize(o)
        | Rvalue::HashCode(o)
        | Rvalue::ToString(o)
        | Rvalue::ArrayLen(o)
        | Rvalue::Discriminant(o)
        | Rvalue::IsType(o, _)
        | Rvalue::EnumName { value: o, .. }
        | Rvalue::ToBytes { value: o, .. }
        | Rvalue::FromBytes { bytes: o, .. }
        | Rvalue::Cast(o, _, _)
        | Rvalue::ArrayNew { len: o, .. } => operand_uses_local(o, local),
        Rvalue::Binary(_, a, b)
        | Rvalue::CharAt(a, b)
        | Rvalue::ByteAt(a, b)
        | Rvalue::Concat(a, b)
        | Rvalue::ArrayRealloc {
            array: a, new_len: b, ..
        } => operand_uses_local(a, local) || operand_uses_local(b, local),
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            operand_uses_local(cond, local)
                || operand_uses_local(then_val, local)
                || operand_uses_local(else_val, local)
        }
        Rvalue::Call { args, .. }
        | Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::Tuple { elems: args, .. }
        | Rvalue::ArrayLit { elems: args, .. } => args.iter().any(|a| operand_uses_local(a, local)),
        Rvalue::IndirectCall { target, args, .. } => {
            operand_uses_local(target, local) || args.iter().any(|a| operand_uses_local(a, local))
        }
        Rvalue::InterfaceCall {
            receiver, args, ..
        } => {
            operand_uses_local(receiver, local) || args.iter().any(|a| operand_uses_local(a, local))
        }
        Rvalue::UnionField { base, .. } => operand_uses_local(base, local),
        Rvalue::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            operand_uses_local(target, local)
                || via.as_ref().is_some_and(|v| operand_uses_local(v, local))
                || method.as_ref().is_some_and(|m| operand_uses_local(m, local))
                || args.iter().any(|(a, _)| operand_uses_local(a, local))
        }
        Rvalue::FuncRef(_) => false,
    }
}

fn is_borrow_local(func: &MirFunction, i: usize) -> bool {
    let d = &func.locals[i];
    if d.is_ref || d.name.as_deref() == Some("this") {
        return true;
    }
    func.params
        .iter()
        .any(|p| p.0 as usize == i && !func.locals[p.0 as usize].is_move)
}

fn exit_keeps(term: &Terminator, local: u32) -> bool {
    match term {
        Terminator::Return(Some(Operand::Copy(Place::Local(l))))
        | Terminator::AsyncComplete(Some(Operand::Copy(Place::Local(l)))) => l.0 == local,
        Terminator::TailCall { args, .. } => args
            .iter()
            .any(|a| matches!(a, Operand::Copy(Place::Local(l)) if l.0 == local)),
        _ => false,
    }
}

pub(crate) fn escape_info(mir: &Mir, interner: &TypeInterner) -> EscapeInfo {
    let mut info = EscapeInfo {
        params: IndexMap::new(),
        ret_alias: IndexMap::new(),
    };
    for f in &mir.functions {
        info.params
            .insert((f.def, f.instance.clone()), vec![false; f.params.len()]);
        info.ret_alias.insert((f.def, f.instance.clone()), false);
    }
    loop {
        let mut grew = false;
        for f in &mir.functions {
            let escaped = locals_escaped(f, f.locals.len(), &info, interner);
            if func_returns_borrow_alias(f, interner) {
                if let Some(slot) = info.ret_alias.get_mut(&(f.def, f.instance.clone())) {
                    if !*slot {
                        *slot = true;
                        grew = true;
                    }
                }
            }
            if let Some(slots) = info.params.get_mut(&(f.def, f.instance.clone())) {
                let skip_this = this_heap_param(f, interner).is_some();
                for (i, p) in f.params.iter().enumerate() {
                    if skip_this && i == 0 {
                        continue;
                    }
                    let idx = p.0 as usize;
                    if idx < escaped.len() && escaped[idx] && !slots[i] {
                        slots[i] = true;
                        grew = true;
                    }
                }
            }
        }
        if !grew {
            break;
        }
    }
    info
}

fn this_heap_param(func: &MirFunction, interner: &TypeInterner) -> Option<crate::Local> {
    let p = func.params.first()?;
    if func.locals[p.0 as usize].name.as_deref() != Some("this") {
        return None;
    }
    let ty = func.locals[p.0 as usize].ty;
    if interner.is_value_type(ty) || interner.is_ref_struct_type(ty) || !interner.is_reference(ty) {
        return None;
    }
    Some(*p)
}

fn func_returns_borrow_alias(func: &MirFunction, interner: &TypeInterner) -> bool {
    let aliases = this_borrow_aliases(func, interner);
    for block in &func.blocks {
        if let Terminator::Return(Some(Operand::Copy(Place::Local(ret))))
        | Terminator::AsyncComplete(Some(Operand::Copy(Place::Local(ret)))) = &block.terminator
        {
            let i = ret.0 as usize;
            if i < aliases.len() && aliases[i] {
                return true;
            }
            if func.params.iter().any(|p| p.0 == ret.0) {
                return true;
            }
        }
    }
    false
}

fn this_borrow_aliases(func: &MirFunction, interner: &TypeInterner) -> Vec<bool> {
    let mut aliases = vec![false; func.locals.len()];
    let Some(this) = this_heap_param(func, interner) else {
        return aliases;
    };
    mark_local(&mut aliases, this);
    let mut moved_out: Vec<(crate::Local, usize)> = Vec::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Statement::AssignNoDrop(
                Place::Field { base, field },
                Rvalue::Use(Operand::Const(crate::Const::Null)),
            ) = stmt
            {
                moved_out.push((*base, *field));
            }
        }
    }
    loop {
        let mut grew = propagate_copy_aliases(func, &mut aliases);
        for block in &func.blocks {
            for stmt in &block.stmts {
                if let Statement::Assign(Place::Local(d), rv)
                | Statement::AssignNoDrop(Place::Local(d), rv) = stmt
                {
                    match rv {
                        Rvalue::Use(Operand::Copy(Place::Field { base, field })) => {
                            if moved_out.iter().any(|(b, f)| b == base && f == field) {
                                continue;
                            }
                            if aliases.get(base.0 as usize).copied().unwrap_or(false) {
                                grew |= mark_local(&mut aliases, *d);
                            }
                        }
                        Rvalue::Use(Operand::Copy(Place::Index { base, .. }))
                        | Rvalue::Use(Operand::Copy(Place::Deref { ptr: base, .. }))
                            if aliases.get(base.0 as usize).copied().unwrap_or(false) =>
                        {
                            grew |= mark_local(&mut aliases, *d);
                        }
                        _ => {}
                    }
                }
            }
        }
        if !grew {
            break;
        }
    }
    aliases
}

fn mark_local(escaped: &mut [bool], l: crate::Local) -> bool {
    let i = l.0 as usize;
    if i < escaped.len() && !escaped[i] {
        escaped[i] = true;
        true
    } else {
        false
    }
}

fn mark_operand(escaped: &mut [bool], o: &Operand) -> bool {
    if let Operand::Copy(Place::Local(l)) = o {
        mark_local(escaped, *l)
    } else {
        false
    }
}

fn lookup_params<'a>(
    map: &'a ParamEscapes,
    def: DefId,
    instance: &[TypeId],
) -> Option<&'a [bool]> {
    map.get(&(def, instance.to_vec()))
        .map(|v| v.as_slice())
        .or_else(|| {
            map.iter()
                .find(|((d, _), _)| *d == def)
                .map(|(_, v)| v.as_slice())
        })
}

fn lookup_ret(map: &RetAlias, def: DefId, instance: &[TypeId]) -> bool {
    map.get(&(def, instance.to_vec()))
        .copied()
        .or_else(|| {
            map.iter()
                .find(|((d, _), _)| *d == def)
                .map(|(_, v)| *v)
        })
        .unwrap_or(false)
}

fn mark_callee_args(
    escaped: &mut [bool],
    callee: &Callee,
    args: &[Operand],
    info: &EscapeInfo,
) -> bool {
    let Some(slots) = lookup_params(&info.params, callee.def, &callee.args) else {
        return false;
    };
    let mut grew = false;
    for (i, a) in args.iter().enumerate() {
        if slots.get(i).copied().unwrap_or(false) {
            grew |= mark_operand(escaped, a);
        }
    }
    grew
}

fn mark_new_ctor_args(
    escaped: &mut [bool],
    ctor: Option<DefId>,
    args: &[Operand],
    info: &EscapeInfo,
) -> bool {
    let Some(ctor) = ctor else {
        return false;
    };
    let Some(slots) = lookup_params(&info.params, ctor, &[]) else {
        return false;
    };
    let mut grew = false;
    for (i, a) in args.iter().enumerate() {
        if slots.get(i + 1).copied().unwrap_or(false) {
            grew |= mark_operand(escaped, a);
        }
    }
    grew
}

fn place_base(place: &Place) -> Option<crate::Local> {
    match place {
        Place::Field { base, .. } | Place::Index { base, .. } => Some(*base),
        Place::Deref { ptr, .. } => Some(*ptr),
        _ => None,
    }
}

/// Borrow parameters and copies of them must not be dropped, but a read-only borrow does not
/// mean the *caller* lost ownership (see `is_word_pairs(ranges)` vs `alloc_leaf(inst)`).
fn drop_aliases(
    func: &MirFunction,
    nlocals: usize,
    info: &EscapeInfo,
    interner: &TypeInterner,
) -> Vec<bool> {
    let mut escaped = locals_escaped(func, nlocals, info, interner);
    for (i, d) in func.locals.iter().enumerate() {
        if d.is_ref
            || d.name.as_deref() == Some("this")
            || func.params.iter().any(|p| p.0 as usize == i && !d.is_move)
        {
            mark_local(&mut escaped, crate::Local(i as u32));
        }
    }
    for block in &func.blocks {
        for stmt in &block.stmts {
            match stmt {
                Statement::Assign(place, rv) | Statement::AssignNoDrop(place, rv) => match rv {
                    Rvalue::Call { callee, .. } => {
                        if let Place::Local(d) = place {
                            if lookup_ret(&info.ret_alias, callee.def, &callee.args) {
                                mark_local(&mut escaped, *d);
                            }
                        }
                    }
                    Rvalue::UnionNew { args, .. } => {
                        for a in args {
                            mark_operand(&mut escaped, a);
                        }
                    }
                    Rvalue::Use(Operand::Copy(
                        Place::Field { .. } | Place::Index { .. } | Place::Deref { .. },
                    )) => {
                        if let Place::Local(d) = place {
                            mark_local(&mut escaped, *d);
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
    let _ = propagate_copy_forward(func, &mut escaped);
    escaped
}

fn locals_escaped(
    func: &MirFunction,
    nlocals: usize,
    info: &EscapeInfo,
    interner: &TypeInterner,
) -> Vec<bool> {
    let mut escaped = vec![false; nlocals];
    if let Some(this) = this_heap_param(func, interner) {
        mark_local(&mut escaped, this);
    }
    let mut moved_out_fields: Vec<(crate::Local, usize)> = Vec::new();
    for block in &func.blocks {
        if let Terminator::Return(Some(Operand::Copy(Place::Local(l))))
        | Terminator::AsyncComplete(Some(Operand::Copy(Place::Local(l)))) = &block.terminator
        {
            mark_local(&mut escaped, *l);
        }
        for stmt in &block.stmts {
            if let Statement::AssignNoDrop(
                Place::Field { base, field },
                Rvalue::Use(Operand::Const(crate::Const::Null)),
            ) = stmt
            {
                moved_out_fields.push((*base, *field));
            }
        }
    }
    loop {
        let mut grew = false;
        grew |= propagate_copy_aliases(func, &mut escaped);
        for block in &func.blocks {
            if let Terminator::TailCall { callee, args } = &block.terminator {
                grew |= mark_callee_args(&mut escaped, callee, args, info);
            }
            for stmt in &block.stmts {
                match stmt {
                    Statement::Call { callee, args } => {
                        grew |= mark_callee_args(&mut escaped, callee, args, info);
                    }
                    Statement::Assign(place, rv) | Statement::AssignNoDrop(place, rv) => {
                        match rv {
                            Rvalue::Call { callee, args } => {
                                grew |= mark_callee_args(&mut escaped, callee, args, info);
                            }
                            Rvalue::New { ctor, args, .. } => {
                                grew |= mark_new_ctor_args(&mut escaped, *ctor, args, info);
                            }
                            Rvalue::Use(Operand::Copy(Place::Field { base, .. })) => {
                                if let Place::Local(d) = place {
                                    if escaped.get(base.0 as usize).copied().unwrap_or(false) {
                                        grew |= mark_local(&mut escaped, *d);
                                    }
                                }
                            }
                            _ => {}
                        }
                        if let Some(base) = place_base(place) {
                            if escaped.get(base.0 as usize).copied().unwrap_or(false) {
                                if let Rvalue::Use(o) = rv {
                                    grew |= mark_operand(&mut escaped, o);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if !moved_out_fields.is_empty() {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    if let Statement::Assign(
                        Place::Local(d),
                        Rvalue::Use(Operand::Copy(Place::Field { base, field })),
                    )
                    | Statement::AssignNoDrop(
                        Place::Local(d),
                        Rvalue::Use(Operand::Copy(Place::Field { base, field })),
                    ) = stmt
                    {
                        if moved_out_fields.iter().any(|(b, f)| b == base && f == field) {
                            grew |= mark_local(&mut escaped, *d);
                        }
                    }
                }
            }
        }
        if !grew {
            break;
        }
    }
    escaped
}

fn propagate_copy_forward(func: &MirFunction, escaped: &mut [bool]) -> bool {
    let mut changed = false;
    let mut grew = true;
    while grew {
        grew = false;
        for block in &func.blocks {
            for stmt in &block.stmts {
                let (Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Local(s))))
                | Statement::AssignNoDrop(
                    Place::Local(d),
                    Rvalue::Use(Operand::Copy(Place::Local(s))),
                )) = stmt
                else {
                    continue;
                };
                let di = d.0 as usize;
                let si = s.0 as usize;
                if di >= escaped.len() || si >= escaped.len() {
                    continue;
                }
                if escaped[si] && !escaped[di] {
                    escaped[di] = true;
                    grew = true;
                    changed = true;
                }
            }
        }
    }
    changed
}

fn propagate_copy_aliases(func: &MirFunction, escaped: &mut [bool]) -> bool {
    let mut changed = false;
    let mut grew = true;
    while grew {
        grew = false;
        for block in &func.blocks {
            for stmt in &block.stmts {
                let (Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Local(s))))
                | Statement::AssignNoDrop(
                    Place::Local(d),
                    Rvalue::Use(Operand::Copy(Place::Local(s))),
                )) = stmt
                else {
                    continue;
                };
                let di = d.0 as usize;
                let si = s.0 as usize;
                if di >= escaped.len() || si >= escaped.len() {
                    continue;
                }
                if escaped[si] && !escaped[di] {
                    escaped[di] = true;
                    grew = true;
                    changed = true;
                }
                if escaped[di] && !escaped[si] {
                    escaped[si] = true;
                    grew = true;
                    changed = true;
                }
            }
        }
    }
    changed
}
