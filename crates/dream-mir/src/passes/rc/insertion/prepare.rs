use super::super::{
    cursor,
    liveness::{self, live_after_stmt},
    modref::ModRefTable,
    tokens::{
        funcbox_env_rc_roots, is_owned_local, leftover_alias_parent, leftover_keep, TokenAnalysis,
    },
};
use crate::{Global, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, TypeInterner};
use indexmap::{IndexMap, IndexSet};
/// Coarse container-slot identity: field slots by `(base, field)`, index slots by base alone
/// (dynamic indices are indistinguishable statically). Used only where a liveness guard makes
/// over-matching safe.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum SlotId {
    Local(u32),
    Global(Global),
    Field(u32, u32),
    IndexBase(u32),
}

fn slot_id(place: &Place) -> SlotId {
    match place {
        Place::Local(l) => SlotId::Local(l.0),
        Place::Global(g) => SlotId::Global(*g),
        Place::Field { base, field } => SlotId::Field(base.0, *field as u32),
        Place::Index { base, .. } => SlotId::IndexBase(base.0),
        Place::Deref { ptr, .. } => SlotId::Local(ptr.0),
    }
}

pub(super) struct State {
    pub(super) local_is_ref: Vec<bool>,
    pub(super) analysis: TokenAnalysis,
    pub(super) leftover_parent: IndexMap<u32, u32>,
    pub(super) env_defer: IndexSet<u32>,
    pub(super) start_keep: Vec<IndexSet<u32>>,
    pub(super) end_keep: Vec<IndexSet<u32>>,
    pub(super) die_keep: IndexMap<(usize, usize), IndexSet<u32>>,
    pub(super) n_orig: u32,
    pub(super) owned_flags: Vec<bool>,
    pub(super) realloc_readers: IndexMap<(usize, usize), Vec<u32>>,
    pub(super) live_out_rc: Vec<IndexSet<u32>>,
    pub(super) is_async: bool,
    pub(super) resume_futures: Vec<IndexSet<u32>>,
    pub(super) in_loop: IndexSet<usize>,
}
impl State {
    pub(super) fn new(
        func: &mut MirFunction,
        interner: &TypeInterner,
        layouts: &dream_hir::LayoutTable,
        holds: &IndexSet<DefId>,
        modref: &ModRefTable,
        analyses: &mut crate::passes::FunctionAnalyses,
    ) -> Self {
        cursor::infer_cursors(func, interner, layouts, modref);

        let local_is_ref: Vec<bool> = func
            .locals
            .iter()
            .map(|d| interner.is_rc_tracked(d.ty))
            .collect();
        let analysis = TokenAnalysis::analyze(func, interner, layouts, holds, modref, analyses);
        let leftover_parent = leftover_alias_parent(func, interner, true);
        let env_defer = funcbox_env_rc_roots(func, interner);
        let start_keep: Vec<IndexSet<u32>> = analysis
            .start_release
            .iter()
            .map(|s| leftover_keep(func, s.iter().copied()))
            .collect();
        let end_keep: Vec<IndexSet<u32>> = analysis
            .end_release
            .iter()
            .map(|s| leftover_keep(func, s.iter().copied()))
            .collect();
        let mut die_keep: IndexMap<(usize, usize), IndexSet<u32>> = IndexMap::new();
        {
            let mut groups: IndexMap<(usize, usize), Vec<u32>> = IndexMap::new();
            for &(bi, si, local) in &analysis.die_after {
                groups.entry((bi, si)).or_default().push(local);
            }
            for (k, ids) in groups {
                die_keep.insert(k, leftover_keep(func, ids));
            }
        }
        let n_orig = func.locals.len() as u32;
        let owned_flags: Vec<bool> = (0..func.locals.len() as u32)
            .map(|l| is_owned_local(func, interner, l))
            .collect();
        let is_owned = |l: u32| owned_flags.get(l as usize).copied().unwrap_or(false);

        let mut realloc_readers: IndexMap<(usize, usize), Vec<u32>> = IndexMap::new();
        let mut slot_readers: IndexMap<SlotId, Vec<u32>> = IndexMap::new();
        let live_out_rc = liveness::live_out(func);
        let is_async = func.is_async;
        // Resume blocks get their awaited future's Release from `insert_await_resume_releases`,
        // which runs after this loop and treats an existing `x = null` as "already handled".
        let mut resume_futures: Vec<IndexSet<u32>> = vec![IndexSet::new(); func.blocks.len()];
        for block in &func.blocks {
            if let Terminator::Await {
                future: Operand::Copy(Place::Local(f)),
                resume,
                ..
            } = &block.terminator
            {
                resume_futures[resume.0 as usize].insert(f.0);
            }
        }
        let in_loop: IndexSet<usize> = analyses
            .natural_loops(func)
            .iter()
            .flat_map(|lp| lp.body.iter().map(|b| b.0 as usize))
            .collect();
        for (bi, block) in func.blocks.iter().enumerate() {
            for (si, stmt) in block.stmts.iter().enumerate() {
                // Collect locals defined by a direct container read, keyed by source slot. A
                // self-realloc of that slot (`f = Buffer.realloc(f, ..)`) consumes the old block
                // outright, so any token still held by such a reader must be dropped *before*
                // the store (see main loop).
                if let Statement::Assign(Place::Local(dest), rv) = stmt {
                    let read_place = match rv {
                        Rvalue::Use(Operand::Copy(p)) | Rvalue::Cast(Operand::Copy(p), _, _) => {
                            Some(p)
                        }
                        _ => None,
                    };
                    if let Some(Place::Field { .. } | Place::Index { .. }) = read_place
                        && interner.is_rc_tracked(func.locals[dest.0 as usize].ty) {
                            slot_readers
                                .entry(slot_id(read_place.unwrap()))
                                .or_default()
                                .push(dest.0);
                        }
                }
                let Statement::Assign(dest_place, Rvalue::ArrayRealloc { array, .. }) = stmt else {
                    continue;
                };
                let Operand::Copy(src_place) = array else {
                    continue;
                };
                if slot_id(dest_place) != slot_id(src_place) {
                    continue;
                }
                let Some(readers) = slot_readers.get(&slot_id(src_place)) else {
                    continue;
                };
                let ok: Vec<u32> = readers
                    .iter()
                    .copied()
                    .filter(|&x| is_owned(x) && !live_after_stmt(func, &live_out_rc, bi, si, x))
                    .collect();
                if !ok.is_empty() {
                    realloc_readers.insert((bi, si), ok);
                }
            }
        }

        Self {
            local_is_ref,
            analysis,
            leftover_parent,
            env_defer,
            start_keep,
            end_keep,
            die_keep,
            n_orig,
            owned_flags,
            realloc_readers,
            live_out_rc,
            is_async,
            resume_futures,
            in_loop,
        }
    }
}
