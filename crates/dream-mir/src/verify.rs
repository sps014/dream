//! Debug-build MIR verifier, run on the final module by [`crate::passes::run_late_module_passes`]
//! when the compiler itself is built with `debug_assertions`. A violation is an ICE.
//!
//! The checks are deliberately conservative so they never fire on correct RC placement:
//!
//! - **RC op on a non-RC local**: `Retain`/`Release`/`ReleaseUnique` of a local whose type is not
//!   RC-tracked.
//! - **Use after `ReleaseUnique`**: `ReleaseUnique x` frees `x`'s graph unconditionally, so any
//!   later read of `x` on the same straight-line path (before `x` is redefined) is a use after free.
//! - **Use / double release after `Release`** (token discipline): only for locals whose
//!   copy/cast/move/niche-payload alias class is never retained, never a parameter, never stored
//!   into a container, and never handed to a call, constructor, or runtime helper. Such a local
//!   holds at most the one token its definition produced, so reading it after `Release x` uses a
//!   value nothing in this function owns any more (a use-after-free unless some unrelated owner
//!   happens to keep the object alive).
//!
//! Limits: the path checks are intra-block (no CFG dataflow, so a release in one block and a use
//! in a successor is not caught), aliases are tracked only by local-to-local copies, and there is
//! no global token-balance check — ownership moves through calls, returns, container stores and
//! inlined callee epilogues cannot be summarized soundly without the RC inserter's own token
//! analysis, so a count-based balance check would either be unsound or fire on correct code.

use crate::{BasicBlock, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::BTreeSet;

/// One verifier finding, located by function, block, and statement index (`stmts.len()` = the
/// terminator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub func: String,
    pub block: usize,
    pub stmt: usize,
    pub msg: String,
}

pub fn verify_module(mir: &Mir, interner: &TypeInterner) -> Vec<Violation> {
    mir.functions
        .iter()
        .chain(mir.polls.iter())
        .flat_map(|f| verify_function(f, interner))
        .collect()
}

/// Panics (an ICE) listing every violation in `mir`.
pub fn assert_module(mir: &Mir, interner: &TypeInterner) {
    let found = verify_module(mir, interner);
    if found.is_empty() {
        return;
    }
    let lines: Vec<String> = found
        .iter()
        .map(|v| format!("  {} bb{}[{}]: {}", v.func, v.block, v.stmt, v.msg))
        .collect();
    crate::internal_error!("MIR verifier failed:\n{}", lines.join("\n"));
}

pub fn verify_function(f: &MirFunction, interner: &TypeInterner) -> Vec<Violation> {
    let mut out = Vec::new();
    let single_token = single_token_locals(f);
    for (bi, block) in f.blocks.iter().enumerate() {
        check_rc_types(f, interner, bi, block, &mut out);
        check_block_paths(f, bi, block, &single_token, &mut out);
    }
    out
}

fn rc_local(op: &Operand) -> Option<Local> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(*l),
        _ => None,
    }
}

fn check_rc_types(
    f: &MirFunction,
    interner: &TypeInterner,
    bi: usize,
    block: &BasicBlock,
    out: &mut Vec<Violation>,
) {
    for (si, s) in block.stmts.iter().enumerate() {
        let (op, what) = match s {
            Statement::Retain(o) => (o, "retain"),
            Statement::Release(o) => (o, "release"),
            Statement::ReleaseUnique(o) => (o, "release_unique"),
            _ => continue,
        };
        let Some(l) = rc_local(op) else { continue };
        let Some(decl) = f.locals.get(l.0 as usize) else {
            out.push(violation(f, bi, si, format!("{what} of undeclared local _{}", l.0)));
            continue;
        };
        if !interner.is_rc_tracked(decl.ty) {
            out.push(violation(
                f,
                bi,
                si,
                format!("{what} of non-RC local _{}", l.0),
            ));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dead {
    Released,
    Destroyed,
}

fn check_block_paths(
    f: &MirFunction,
    bi: usize,
    block: &BasicBlock,
    single_token: &BTreeSet<u32>,
    out: &mut Vec<Violation>,
) {
    let mut dead: Vec<(u32, Dead)> = Vec::new();
    for (si, s) in block.stmts.iter().enumerate() {
        for &(l, how) in &dead {
            if !crate::passes::stmt_reads_local(s, l) {
                continue;
            }
            let msg = match (how, s) {
                (Dead::Released, Statement::Release(_) | Statement::ReleaseUnique(_)) => {
                    format!("_{l} released twice with no retain or redefinition between")
                }
                (Dead::Released, _) => format!("_{l} used after its only token was released"),
                (Dead::Destroyed, _) => format!("_{l} used after release_unique"),
            };
            out.push(violation(f, bi, si, msg));
        }
        if let Some(d) = defined_local(s) {
            dead.retain(|(l, _)| *l != d);
        }
        match s {
            Statement::ReleaseUnique(o) => {
                if let Some(l) = rc_local(o) {
                    mark(&mut dead, l.0, Dead::Destroyed);
                }
            }
            Statement::Release(o) => {
                if let Some(l) = rc_local(o) {
                    if single_token.contains(&l.0) {
                        mark(&mut dead, l.0, Dead::Released);
                    }
                }
            }
            _ => {}
        }
    }
    let term_reads = terminator_reads(&block.terminator);
    for &(l, how) in &dead {
        if term_reads.contains(&l) {
            let msg = match how {
                Dead::Released => format!("_{l} used by terminator after its only token was released"),
                Dead::Destroyed => format!("_{l} used by terminator after release_unique"),
            };
            out.push(violation(f, bi, block.stmts.len(), msg));
        }
    }
}

fn mark(dead: &mut Vec<(u32, Dead)>, l: u32, how: Dead) {
    dead.retain(|(d, _)| *d != l);
    dead.push((l, how));
}

fn defined_local(s: &Statement) -> Option<u32> {
    match s {
        Statement::Assign(Place::Local(d), _) => Some(d.0),
        _ => None,
    }
}

fn violation(f: &MirFunction, block: usize, stmt: usize, msg: String) -> Violation {
    Violation {
        func: f.name.clone(),
        block,
        stmt,
        msg,
    }
}

/// Locals whose whole copy/cast/move alias class never gains a second owner: no member is
/// retained, stored anywhere but a plain local, handed to a call/constructor/union/array, or is a
/// parameter. Every definition of such a local yields at most one token.
fn single_token_locals(f: &MirFunction) -> BTreeSet<u32> {
    let n = f.locals.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let union = |parent: &mut Vec<usize>, a: u32, b: u32| {
        let (a, b) = (a as usize, b as usize);
        if a >= n || b >= n {
            return;
        }
        let (ra, rb) = (find(parent, a), find(parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    };
    let mut shared: BTreeSet<u32> = f.params.iter().map(|p| p.0).collect();
    for block in &f.blocks {
        for s in &block.stmts {
            match s {
                Statement::Retain(o) => {
                    if let Some(l) = rc_local(o) {
                        shared.insert(l.0);
                    }
                }
                Statement::Assign(Place::Local(d), rv) => match rv {
                    Rvalue::Use(Operand::Copy(Place::Local(s)))
                    | Rvalue::Cast(Operand::Copy(Place::Local(s)), _, _)
                    | Rvalue::Move { src: s, .. } => union(&mut parent, d.0, s.0),
                    Rvalue::Select {
                        then_val, else_val, ..
                    } => {
                        for l in [then_val, else_val].iter().filter_map(|o| rc_local(o)) {
                            union(&mut parent, d.0, l.0);
                        }
                    }
                    Rvalue::UnionField { base, .. } => {
                        if let Some(b) = rc_local(base) {
                            union(&mut parent, d.0, b.0);
                        }
                    }
                    Rvalue::Use(_)
                    | Rvalue::Cast(..)
                    | Rvalue::Binary(..)
                    | Rvalue::CheckedBinary(..)
                    | Rvalue::Unary(..)
                    | Rvalue::CheckedNeg(_)
                    | Rvalue::Discriminant { .. }
                    | Rvalue::IsType(..)
                    | Rvalue::ArrayLen(_)
                    | Rvalue::StrLen(_)
                    | Rvalue::StrByteSize(_)
                    | Rvalue::CharAt(..)
                    | Rvalue::ByteAt(..)
                    | Rvalue::StrBytes(_)
                    | Rvalue::LoadU8(..)
                    | Rvalue::LoadU16(..)
                    | Rvalue::HashCode(_)
                    | Rvalue::EnumName { .. } => {}
                    _ => shared.extend(rvalue_local_operands(rv)),
                },
                Statement::Assign(_, rv) => shared.extend(rvalue_local_operands(rv)),
                Statement::Release(_)
                | Statement::ReleaseUnique(_)
                | Statement::Nop
                | Statement::DebugLine(_)
                | Statement::SourceLine(_) => {}
                other => shared.extend(other_stmt_locals(other)),
            }
        }
        match &block.terminator {
            Terminator::TailCall { args, .. } => {
                shared.extend(args.iter().filter_map(rc_local).map(|l| l.0));
            }
            Terminator::Await { dest: Some(d), .. } => {
                shared.insert(d.0);
            }
            _ => {}
        }
    }
    let shared_roots: BTreeSet<usize> = shared
        .iter()
        .filter(|l| (**l as usize) < n)
        .map(|l| find(&mut parent, *l as usize))
        .collect();
    (0..n)
        .filter(|l| !shared_roots.contains(&find(&mut parent, *l)))
        .map(|l| l as u32)
        .collect()
}

fn operand_locals(o: &Operand, out: &mut Vec<u32>) {
    if let Operand::Copy(p) = o {
        match p {
            Place::Local(l) => out.push(l.0),
            Place::Field { base, .. } | Place::Deref { ptr: base, .. } => out.push(base.0),
            Place::Index { base, index, .. } => {
                out.push(base.0);
                operand_locals(index, out);
            }
            Place::Global(_) => {}
        }
    }
}

fn rvalue_local_operands(rv: &Rvalue) -> Vec<u32> {
    let mut out = Vec::new();
    let mut ops: Vec<&Operand> = Vec::new();
    match rv {
        Rvalue::Move { src, .. } => out.push(src.0),
        Rvalue::Use(o)
        | Rvalue::Unary(_, o)
        | Rvalue::CheckedNeg(o)
        | Rvalue::ArrayLen(o)
        | Rvalue::StrLen(o)
        | Rvalue::StrByteSize(o)
        | Rvalue::StrBytes(o)
        | Rvalue::Cast(o, _, _)
        | Rvalue::IsType(o, _)
        | Rvalue::TypeName(o)
        | Rvalue::Discriminant { base: o, .. }
        | Rvalue::HashCode(o)
        | Rvalue::ToString(o)
        | Rvalue::UnionField { base: o, .. }
        | Rvalue::EnumName { value: o, .. }
        | Rvalue::ArrayNew { len: o, .. }
        | Rvalue::ToBytes { value: o, .. }
        | Rvalue::FromBytes { bytes: o, .. } => ops.push(o),
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => ops.extend([cond, then_val, else_val]),
        Rvalue::Binary(_, a, b)
        | Rvalue::CheckedBinary(_, a, b)
        | Rvalue::CharAt(a, b, _)
        | Rvalue::ByteAt(a, b, _)
        | Rvalue::LoadU8(a, b)
        | Rvalue::LoadU16(a, b)
        | Rvalue::ArrayRealloc {
            array: a,
            new_len: b,
            ..
        } => ops.extend([a, b]),
        Rvalue::ConcatInt {
            prefix,
            value,
            suffix,
        } => ops.extend([prefix, value, suffix]),
        Rvalue::Concat(args)
        | Rvalue::Call { args, .. }
        | Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::ArrayLit { elems: args, .. }
        | Rvalue::Tuple { elems: args, .. } => ops.extend(args.iter()),
        Rvalue::IndirectCall { target, args, .. } => {
            ops.push(target);
            ops.extend(args.iter());
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            ops.push(receiver);
            ops.extend(args.iter());
        }
        Rvalue::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            ops.push(target);
            ops.extend(via.iter());
            ops.extend(method.iter());
            ops.extend(args.iter().map(|(a, _)| a));
        }
        Rvalue::FuncRef(_) => {}
    }
    for o in ops {
        operand_locals(o, &mut out);
    }
    out
}

fn terminator_reads(t: &Terminator) -> BTreeSet<u32> {
    let mut out = Vec::new();
    match t {
        Terminator::If { cond: o, .. }
        | Terminator::Switch { value: o, .. }
        | Terminator::Return(Some(o))
        | Terminator::AsyncComplete(Some(o))
        | Terminator::Await { future: o, .. } => operand_locals(o, &mut out),
        Terminator::TailCall { args, .. } => args.iter().for_each(|a| operand_locals(a, &mut out)),
        _ => {}
    }
    out.into_iter().collect()
}

/// Locals read by a statement that is not an `Assign`/RC op/marker — every one of these
/// (calls, prints, locks, SIMD, value glue, …) may retain, store, or inspect its operands.
fn other_stmt_locals(s: &Statement) -> Vec<u32> {
    let mut out = Vec::new();
    let mut ops: Vec<&Operand> = Vec::new();
    match s {
        Statement::Panic(o)
        | Statement::ForceFree(o)
        | Statement::LockAcquire(o)
        | Statement::LockRelease(o)
        | Statement::DeferLeave(o)
        | Statement::Print { arg: o, .. } => ops.push(o),
        Statement::Call { args, .. } => ops.extend(args.iter()),
        Statement::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            ops.push(target);
            ops.extend(via.iter());
            ops.extend(method.iter());
            ops.extend(args.iter().map(|(a, _)| a));
        }
        Statement::InterfaceCall { receiver, args, .. } => {
            ops.push(receiver);
            ops.extend(args.iter());
        }
        Statement::IndirectCall { target, args, .. } => {
            ops.push(target);
            ops.extend(args.iter());
        }
        Statement::ArrayElemsCopy {
            dst,
            dst_off,
            src,
            src_off,
            count,
            ..
        } => ops.extend([dst, dst_off, src, src_off, count]),
        Statement::ArrayElemsFill {
            dst, dst_off, count, ..
        } => ops.extend([dst, dst_off, count]),
        Statement::SimdV128 {
            dest,
            lhs,
            rhs,
            index,
            splat_rhs,
            ..
        } => {
            ops.extend([dest, lhs, rhs, index]);
            ops.extend(splat_rhs.iter());
        }
        Statement::ValueDrop(l) | Statement::ValueRetain(l) | Statement::ValueKill(l) => {
            out.push(l.0)
        }
        Statement::Assign(..)
        | Statement::Retain(_)
        | Statement::Release(_)
        | Statement::ReleaseUnique(_)
        | Statement::Nop
        | Statement::DebugLine(_)
        | Statement::SourceLine(_)
        | Statement::DeferEnter
        | Statement::RegionEnter
        | Statement::RegionLeave => {}
    }
    for o in ops {
        operand_locals(o, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::{Callee, Const};
    use dream_types::{DefKind, TypeCtx};

    fn node(ctx: &mut TypeCtx) -> dream_types::TypeId {
        let def = ctx.register(DefKind::Struct, "Node", vec![]);
        ctx.interner.struct_ty(def, vec![])
    }

    fn new_node(ty: dream_types::TypeId) -> Rvalue {
        Rvalue::New {
            def: dream_types::DefId(0),
            ty,
            ctor: None,
            args: vec![],
        }
    }

    fn copy(l: Local) -> Operand {
        Operand::Copy(Place::Local(l))
    }

    #[test]
    fn flags_use_after_release_unique() {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let mut f = FunctionBuilder::new("f", ty);
        let x = f.new_local(ty, None);
        f.assign(Place::Local(x), new_node(ty));
        f.push(Statement::ReleaseUnique(copy(x)));
        f.terminate(Terminator::Return(Some(copy(x))));
        let v = verify_function(&f.finish(), &ctx.interner);
        assert_eq!(v.len(), 1, "{:?}", v);
        assert!(v[0].msg.contains("release_unique"), "{:?}", v);
    }

    #[test]
    fn flags_use_and_double_release_of_single_token_local() {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let mut f = FunctionBuilder::new("f", ctx.interner.void());
        let x = f.new_local(ty, None);
        f.assign(Place::Local(x), new_node(ty));
        f.push(Statement::Release(copy(x)));
        f.push(Statement::Print {
            arg: copy(x),
            ty,
            newline: true,
        });
        f.push(Statement::Release(copy(x)));
        f.terminate(Terminator::Return(None));
        let v = verify_function(&f.finish(), &ctx.interner);
        assert_eq!(v.len(), 0, "print shares x, so it is not single-token: {:?}", v);

        let mut f = FunctionBuilder::new("g", ctx.interner.void());
        let x = f.new_local(ty, None);
        let n = f.new_local(ctx.interner.bool(), None);
        f.assign(Place::Local(x), new_node(ty));
        f.push(Statement::Release(copy(x)));
        f.assign(
            Place::Local(n),
            Rvalue::Binary(crate::BinOp::Eq, copy(x), Operand::Const(Const::Null)),
        );
        f.push(Statement::Release(copy(x)));
        f.terminate(Terminator::Return(None));
        let v = verify_function(&f.finish(), &ctx.interner);
        assert_eq!(v.len(), 2, "{:?}", v);
        assert!(v[1].msg.contains("released twice"), "{:?}", v);
    }

    #[test]
    fn retained_or_passed_aliases_are_not_single_token() {
        let mut ctx = TypeCtx::new();
        let ty = node(&mut ctx);
        let mut f = FunctionBuilder::new("f", ctx.interner.void());
        let x = f.new_local(ty, None);
        let y = f.new_local(ty, None);
        let z = f.new_local(ty, None);
        f.assign(Place::Local(x), new_node(ty));
        f.assign(Place::Local(y), Rvalue::Use(copy(x)));
        f.push(Statement::Retain(copy(y)));
        f.push(Statement::Release(copy(x)));
        f.assign(Place::Local(z), Rvalue::Use(copy(x)));
        f.push(Statement::Call {
            callee: Callee {
                def: dream_types::DefId(1),
                args: vec![],
                ret: ctx.interner.void(),
                take_params: vec![],
            },
            args: vec![copy(z)],
        });
        f.terminate(Terminator::Return(None));
        assert!(verify_function(&f.finish(), &ctx.interner).is_empty());
    }

    #[test]
    fn flags_rc_op_on_scalar() {
        let ctx = TypeCtx::new();
        let mut f = FunctionBuilder::new("f", ctx.interner.void());
        let i = f.new_local(ctx.interner.int(), None);
        f.assign(Place::Local(i), Rvalue::Use(Operand::Const(Const::Int(1))));
        f.push(Statement::Retain(copy(i)));
        f.terminate(Terminator::Return(None));
        let v = verify_function(&f.finish(), &ctx.interner);
        assert_eq!(v.len(), 1, "{:?}", v);
        assert!(v[0].msg.contains("non-RC"), "{:?}", v);
    }
}
