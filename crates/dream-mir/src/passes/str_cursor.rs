//! Hoist a string's payload pointer out of a scan loop.
//!
//! `byte_at` / `char_at` re-read the string header on every access (slice check, then the units
//! pointer). A loop that only reads one string otherwise redoes that dependent load per byte, and
//! clang will not turn the scan into a pointer walk while the header load sits in the body.
//! The preheader loads the payload address once; the body indexes that pointer.

use super::MirPass;
use crate::{
    BasicBlock, BlockId, Local, LocalDecl, MirFunction, Operand, Place, Rvalue, Statement,
    Terminator,
};
use dream_types::TypeInterner;
use indexmap::IndexSet as HashSet;
use std::collections::{BTreeMap, BTreeSet};

pub struct StrCursor;

impl MirPass for StrCursor {
    fn preserves(&self) -> crate::passes::PreservedAnalyses {
        crate::passes::PreservedAnalyses::None
    }

    fn name(&self) -> &'static str {
        "str-cursor"
    }

    fn transform(
        &self,
        func: &mut MirFunction,
        interner: &TypeInterner,
        _layouts: &dream_hir::LayoutTable,
        analyses: &mut crate::passes::FunctionAnalyses,
    ) -> bool {
        let mut changed = false;
        let mut skipped: BTreeSet<u32> = BTreeSet::new();
        // Innermost first. A refused loop is skipped so a later, quieter loop can still
        // hoist; block ids of existing loops stay valid when a preheader is inserted.
        for _ in 0..(func.blocks.len() + 1) * 2 {
            let cached_loops = analyses.natural_loops(func);
            let mut loops: Vec<_> = cached_loops.iter().collect();
            loops.sort_by_key(|l| l.body.len());
            let Some(target) = loops
                .into_iter()
                .find(|l| !skipped.contains(&l.header.0) && !scan_bases(func, &l.body).is_empty())
            else {
                break;
            };
            if !apply(func, interner, target.header, &target.body, analyses) {
                skipped.insert(target.header.0);
                continue;
            }
            analyses.invalidate();
            changed = true;
        }
        changed
    }
}

fn apply(
    func: &mut MirFunction,
    interner: &TypeInterner,
    header: BlockId,
    body: &BTreeSet<BlockId>,
    analyses: &mut crate::passes::FunctionAnalyses,
) -> bool {
    let bases = scan_bases(func, body);
    if bases.is_empty() || !body_is_scan(func, interner, body, &bases) {
        return false;
    }
    let checked = checked_bases(func, body);
    let mut cursor_of: BTreeMap<u32, Cursor> = BTreeMap::new();
    let mut setup = Vec::new();
    for base in bases {
        let s = || Operand::Copy(Place::Local(Local(base)));
        let ptr = new_int_temp(func, interner);
        setup.push(Statement::Assign(Place::Local(ptr), Rvalue::StrBytes(s())));
        let len = checked.contains(&base).then(|| {
            let len = new_int_temp(func, interner);
            setup.push(Statement::Assign(Place::Local(len), Rvalue::StrLen(s())));
            len
        });
        cursor_of.insert(base, Cursor { ptr, len });
    }
    for &b in body {
        for stmt in &mut func.block_mut(b).stmts {
            let Statement::Assign(_, rv) = stmt else {
                continue;
            };
            rewrite_rvalue(rv, &cursor_of);
        }
    }
    let incoming: Vec<BlockId> = analyses
        .predecessors(func)
        .get(header.0 as usize)
        .into_iter()
        .flatten()
        .copied()
        .filter(|p| !body.contains(p))
        .collect();
    let ph = BlockId(func.blocks.len() as u32);
    func.blocks.push(BasicBlock {
        stmts: setup,
        terminator: Terminator::Goto(header),
    });
    for pred in incoming {
        redirect(&mut func.blocks[pred.0 as usize].terminator, header, ph);
    }
    if func.entry == header {
        func.entry = ph;
    }
    true
}

/// The hoisted payload pointer, plus the hoisted length when some read was not proven in range.
#[derive(Clone, Copy)]
struct Cursor {
    ptr: Local,
    len: Option<Local>,
}

fn rewrite_rvalue(rv: &mut Rvalue, cursor_of: &BTreeMap<u32, Cursor>) {
    let (Rvalue::ByteAt(s, i, unchecked) | Rvalue::CharAt(s, i, unchecked)) = rv else {
        return;
    };
    let Some(cursor) = base_local(s).and_then(|b| cursor_of.get(&b).copied()) else {
        return;
    };
    let ptr = Operand::Copy(Place::Local(cursor.ptr));
    let len = if *unchecked {
        None
    } else {
        let len = cursor.len.unwrap_or_else(|| {
            crate::internal_error!("str-cursor: checked read without a hoisted length")
        });
        Some(Operand::Copy(Place::Local(len)))
    };
    let i = i.clone();
    *rv = if matches!(rv, Rvalue::ByteAt(..)) {
        Rvalue::LoadU8(ptr, i, len)
    } else {
        Rvalue::LoadU16(ptr, i, len)
    };
}

fn read_base(stmt: &Statement) -> Option<(u32, bool)> {
    match stmt {
        Statement::Assign(_, Rvalue::ByteAt(s, _, unchecked) | Rvalue::CharAt(s, _, unchecked)) => {
            base_local(s).map(|b| (b, *unchecked))
        }
        _ => None,
    }
}

/// String locals that are only read, via `byte_at` / `char_at`, inside `body`.
fn scan_bases(func: &MirFunction, body: &BTreeSet<BlockId>) -> BTreeSet<u32> {
    let mut bases = BTreeSet::new();
    let mut defined = HashSet::new();
    for &b in body {
        for stmt in &func.block(b).stmts {
            if let Statement::Assign(Place::Local(d), _) = stmt {
                defined.insert(d.0);
            }
            if let Some((base, _)) = read_base(stmt) {
                bases.insert(base);
            }
        }
    }
    bases.retain(|b| !defined.contains(b));
    bases
}

fn checked_bases(func: &MirFunction, body: &BTreeSet<BlockId>) -> BTreeSet<u32> {
    body.iter()
        .flat_map(|&b| func.block(b).stmts.iter())
        .filter_map(read_base)
        .filter(|&(_, unchecked)| !unchecked)
        .map(|(base, _)| base)
        .collect()
}

fn base_local(op: &Operand) -> Option<u32> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(l.0),
        _ => None,
    }
}

/// The body may read the string and do scalar work. A call, a store, or a release of the string
/// can free or rewrite the payload, so the hoisted pointer would dangle.
fn body_is_scan(
    func: &MirFunction,
    interner: &TypeInterner,
    body: &BTreeSet<BlockId>,
    bases: &BTreeSet<u32>,
) -> bool {
    for &b in body {
        let block = func.block(b);
        for stmt in &block.stmts {
            if !stmt_is_scan(stmt, func, interner, bases) {
                return false;
            }
        }
        match &block.terminator {
            Terminator::Goto(_) | Terminator::If { .. } | Terminator::Switch { .. } => {}
            _ => return false,
        }
    }
    true
}

fn stmt_is_scan(
    stmt: &Statement,
    func: &MirFunction,
    interner: &TypeInterner,
    bases: &BTreeSet<u32>,
) -> bool {
    match stmt {
        Statement::Nop | Statement::DebugLine(_) | Statement::SourceLine(_) => true,
        Statement::Retain(op) => base_local(op).is_none_or(|b| !bases.contains(&b)),
        // A different local may own the same string; local identity alone cannot prove
        // that its final release leaves the hoisted payload alive.
        Statement::Release(_) => false,
        // Clearing a nonescaping borrowed view changes only its frame storage, never
        // the source string's payload or ownership.
        Statement::ValueKill(l) => {
            func.locals[l.0 as usize].borrows_refs
                && !func.locals[l.0 as usize].is_ref
                && interner.is_value_type(func.local_ty(*l))
        }
        Statement::Assign(Place::Local(d), Rvalue::New { ctor: None, .. }) => {
            func.locals[d.0 as usize].borrows_refs && interner.is_value_type(func.local_ty(*d))
        }
        Statement::Assign(Place::Field { base, .. }, rv) => {
            // Borrowing proved these frame-local aliases cannot publish, retain, or release
            // the source while the payload pointer is in use.
            func.locals[base.0 as usize].borrows_refs
                && interner.is_value_type(func.local_ty(*base))
                && rvalue_is_scan(rv)
        }
        Statement::Assign(Place::Local(_), rv) => rvalue_is_scan(rv),
        _ => false,
    }
}

pub(super) fn rvalue_is_scan(rv: &Rvalue) -> bool {
    matches!(
        rv,
        Rvalue::Use(_)
            | Rvalue::Binary(..)
            | Rvalue::Unary(..)
            | Rvalue::CheckedBinary(..)
            | Rvalue::CheckedNeg(_)
            | Rvalue::Select { .. }
            | Rvalue::Cast(..)
            | Rvalue::StrLen(_)
            | Rvalue::StrByteSize(_)
            | Rvalue::StrBytes(_)
            | Rvalue::CharAt(..)
            | Rvalue::ByteAt(..)
            | Rvalue::LoadU8(..)
            | Rvalue::LoadU16(..)
            | Rvalue::Discriminant { .. }
    )
}

fn new_int_temp(func: &mut MirFunction, interner: &TypeInterner) -> Local {
    let id = Local(func.locals.len() as u32);
    func.locals.push(LocalDecl {
        ty: interner.int(),
        name: None,
        is_ref: false,
        is_take: false,
        is_cursor: false,
        manual_drop: false,
        borrows_refs: false,
    });
    id
}

fn redirect(t: &mut Terminator, from: BlockId, to: BlockId) {
    let fix = |b: &mut BlockId| {
        if *b == from {
            *b = to;
        }
    };
    match t {
        Terminator::Goto(b) => fix(b),
        Terminator::If {
            then_blk, else_blk, ..
        } => {
            fix(then_blk);
            fix(else_blk);
        }
        Terminator::Switch {
            targets, default, ..
        } => {
            for (_, b) in targets {
                fix(b);
            }
            fix(default);
        }
        Terminator::Await { resume, .. } => fix(resume),
        Terminator::Return(_)
        | Terminator::AsyncComplete(_)
        | Terminator::TailCall { .. }
        | Terminator::Unreachable => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use dream_types::TypeCtx;

    fn scan(unchecked: bool) -> (MirFunction, BlockId) {
        let ctx = TypeCtx::new();
        let i = &ctx.interner;
        let mut b = FunctionBuilder::new("scan", i.void());
        let s = b.new_param(i.string(), None);
        let idx = b.new_param(i.int(), None);
        let cond = b.new_param(i.bool(), None);
        let unit = b.new_local(i.char(), None);
        let header = b.new_block();
        let body = b.new_block();
        let done = b.new_block();
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(cond)),
            then_blk: body,
            else_blk: done,
        });
        b.switch_to(body);
        b.assign(
            Place::Local(unit),
            Rvalue::CharAt(
                Operand::Copy(Place::Local(s)),
                Operand::Copy(Place::Local(idx)),
                unchecked,
            ),
        );
        b.terminate(Terminator::Goto(header));
        b.switch_to(done);
        b.terminate(Terminator::Return(None));
        let mut f = b.finish();
        assert!(StrCursor.transform(&mut f, i, &Default::default(), &mut Default::default()));
        (f, body)
    }

    fn body_load(f: &MirFunction, body: BlockId) -> &Rvalue {
        f.block(body)
            .stmts
            .iter()
            .find_map(|s| match s {
                Statement::Assign(_, rv @ Rvalue::LoadU16(..)) => Some(rv),
                _ => None,
            })
            .expect("the read becomes a cursor load")
    }

    fn preheader_has(f: &MirFunction, pred: impl Fn(&Rvalue) -> bool) -> bool {
        f.blocks.last().is_some_and(|ph| {
            ph.stmts
                .iter()
                .any(|s| matches!(s, Statement::Assign(_, rv) if pred(rv)))
        })
    }

    #[test]
    fn proven_read_loads_without_a_bound() {
        let (f, body) = scan(true);
        assert!(matches!(body_load(&f, body), Rvalue::LoadU16(_, _, None)));
        assert!(!preheader_has(&f, |rv| matches!(rv, Rvalue::StrLen(_))));
    }

    #[test]
    fn checked_read_keeps_its_bound_against_a_hoisted_length() {
        let (f, body) = scan(false);
        assert!(matches!(body_load(&f, body), Rvalue::LoadU16(_, _, Some(_))));
        assert!(preheader_has(&f, |rv| matches!(rv, Rvalue::StrLen(_))));
        assert!(preheader_has(&f, |rv| matches!(rv, Rvalue::StrBytes(_))));
        assert!(!crate::passes::dce::is_pure(body_load(&f, body)));
    }
}
