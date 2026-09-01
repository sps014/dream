//! Inferred unique-graph bump region: wrap `x = f(...); … ReleaseUnique x` when `f` only allocates
//! `del`-free class instances that cannot escape the region. The runtime TLS slab then rewinds in
//! O(1) instead of walking/recycling each node.

use super::ModulePass;
use crate::{
    Callee, Const, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator,
};
use dream_types::{DefId, TypeId, TypeInterner};
use std::collections::{HashMap, HashSet};

pub struct UniqueRegion;

struct SafeCx<'a> {
    mir: &'a Mir,
    interner: &'a TypeInterner,
    ctor_only: &'a HashSet<DefId>,
    memo: &'a mut HashMap<(DefId, Vec<TypeId>), bool>,
    visiting: &'a mut HashSet<(DefId, Vec<TypeId>)>,
}

impl ModulePass for UniqueRegion {
    fn name(&self) -> &'static str {
        "unique-region"
    }

    fn run(&self, mir: &mut Mir, interner: &TypeInterner) -> bool {
        let ctor_only = ctor_only_defs(mir);
        let mut safe: HashMap<(DefId, Vec<TypeId>), bool> = HashMap::new();
        let mut changed = false;
        let n = mir.functions.len();
        for i in 0..n {
            if mir.functions[i].is_async {
                continue;
            }
            let sites = wrap_sites(mir, interner, i, &ctor_only, &mut safe);
            if sites.is_empty() {
                continue;
            }
            apply_wraps(&mut mir.functions[i], &sites);
            changed = true;
        }
        changed
    }
}

/// Drop enter/leave pairs when a later pass merged a payload use (call, to_string, return)
/// after `RegionLeave` without redefining the RC local.
pub fn strip_escaped_regions(mir: &mut Mir, interner: &TypeInterner) -> bool {
    let mut changed = false;
    for f in &mut mir.functions {
        if strip_escaped_fn(f, interner) {
            changed = true;
        }
    }
    changed
}

fn ctor_only_defs(mir: &Mir) -> HashSet<DefId> {
    let mut as_ctor = HashSet::new();
    let mut as_call = HashSet::new();
    for f in &mir.functions {
        walk_fn(f, |stmt| match stmt {
            Statement::Call { callee, .. } => {
                as_call.insert(callee.def);
            }
            Statement::Assign(_, rv) => match rv {
                Rvalue::Call { callee, .. } => {
                    as_call.insert(callee.def);
                }
                Rvalue::New {
                    ctor: Some(ctor), ..
                } => {
                    as_ctor.insert(*ctor);
                }
                _ => {}
            },
            _ => {}
        });
        for b in &f.blocks {
            if let Terminator::TailCall { callee, .. } = &b.terminator {
                as_call.insert(callee.def);
            }
        }
    }
    as_ctor
        .into_iter()
        .filter(|d| !as_call.contains(d))
        .collect()
}

fn walk_fn(f: &MirFunction, mut visit: impl FnMut(&Statement)) {
    for b in &f.blocks {
        for s in &b.stmts {
            visit(s);
        }
    }
}

fn find_fn<'a>(mir: &'a Mir, def: DefId, args: &[TypeId]) -> Option<&'a MirFunction> {
    mir.functions
        .iter()
        .find(|f| f.def == def && f.instance == args)
}

fn has_del(mir: &Mir, layout_name: &str) -> bool {
    let name = format!("{layout_name}_del");
    mir.functions.iter().any(|f| f.name == name)
}

fn region_safe(cx: &mut SafeCx<'_>, f: &MirFunction) -> bool {
    let key = (f.def, f.instance.clone());
    if let Some(v) = cx.memo.get(&key) {
        return *v;
    }
    if !cx.visiting.insert(key.clone()) {
        return true;
    }
    let ok = region_safe_body(cx, f);
    cx.visiting.remove(&key);
    cx.memo.insert(key, ok);
    ok
}

fn region_safe_body(cx: &mut SafeCx<'_>, f: &MirFunction) -> bool {
    if f.is_async {
        return false;
    }
    if cx.mir.intrinsics.iter().any(|(d, _)| *d == f.def) {
        return false;
    }
    let this_local = f.params.first().copied();
    let mut new_locals = HashSet::new();
    for b in &f.blocks {
        for s in &b.stmts {
            if let Statement::Assign(Place::Local(d), Rvalue::New { .. }) = s {
                new_locals.insert(*d);
            }
        }
        match &b.terminator {
            Terminator::TailCall { .. } | Terminator::Await { .. } => return false,
            _ => {}
        }
    }
    for b in &f.blocks {
        for s in &b.stmts {
            if !stmt_region_safe(cx, f, s, this_local, &new_locals) {
                return false;
            }
        }
    }
    true
}

fn callee_safe(cx: &mut SafeCx<'_>, callee: &Callee) -> bool {
    if cx.mir.intrinsics.iter().any(|(d, _)| *d == callee.def) {
        return false;
    }
    let Some(g) = find_fn(cx.mir, callee.def, &callee.args) else {
        return false;
    };
    region_safe(cx, g)
}

fn stmt_region_safe(
    cx: &mut SafeCx<'_>,
    f: &MirFunction,
    stmt: &Statement,
    this_local: Option<Local>,
    new_locals: &HashSet<Local>,
) -> bool {
    match stmt {
        Statement::Nop
        | Statement::DebugLine(_)
        | Statement::SourceLine(_)
        | Statement::Retain(_)
        | Statement::Release(_)
        | Statement::ReleaseUnique(_)
        | Statement::RegionEnter
        | Statement::RegionLeave => true,
        Statement::Print { .. }
        | Statement::Panic(_)
        | Statement::JsCall { .. }
        | Statement::InterfaceCall { .. }
        | Statement::IndirectCall { .. }
        | Statement::ForceFree(_)
        | Statement::LockAcquire(_)
        | Statement::LockRelease(_)
        | Statement::DeferEnter
        | Statement::DeferLeave(_)
        | Statement::Call { .. }
        | Statement::ArrayElemsCopy { .. }
        | Statement::ArrayElemsFill { .. }
        | Statement::SimdV128 { .. }
        | Statement::ValueDrop(_)
        | Statement::ValueRetain(_)
        | Statement::ValueKill(_) => false,
        Statement::Assign(place, rv) => {
            if matches!(
                place,
                Place::Global(_) | Place::Index { .. } | Place::Deref { .. }
            ) {
                return false;
            }
            if let Place::Field { base, .. } = place {
                let ok_base = new_locals.contains(base)
                    || (this_local == Some(*base) && cx.ctor_only.contains(&f.def));
                if !ok_base {
                    return false;
                }
            }
            rvalue_region_safe(cx, rv)
        }
    }
}

fn rvalue_region_safe(cx: &mut SafeCx<'_>, rv: &Rvalue) -> bool {
    match rv {
        Rvalue::Use(_)
        | Rvalue::Select { .. }
        | Rvalue::Binary(_, _, _)
        | Rvalue::Unary(_, _)
        | Rvalue::StrLen(_)
        | Rvalue::StrByteSize(_)
        | Rvalue::CharAt(_, _, _)
        | Rvalue::ByteAt(_, _, _)
        | Rvalue::HashCode(_)
        | Rvalue::ArrayLen(_)
        | Rvalue::Cast(_, _, _)
        | Rvalue::Discriminant { .. }
        | Rvalue::UnionField { .. }
        | Rvalue::IsType(_, _)
        | Rvalue::Tuple { .. }
        | Rvalue::EnumName { .. } => true,
        Rvalue::Call { callee, .. } => callee_safe(cx, callee),
        Rvalue::New { ty, ctor, .. } => {
            let Some(layout) = cx.mir.layouts.get(*ty) else {
                return false;
            };
            if has_del(cx.mir, &layout.name) {
                return false;
            }
            if let Some(ctor) = ctor {
                let Some(g) = cx.mir.functions.iter().find(|cf| cf.def == *ctor) else {
                    return false;
                };
                if !cx.ctor_only.contains(ctor) {
                    return false;
                }
                region_safe(cx, g)
            } else {
                true
            }
        }
        Rvalue::UnionNew { ty, .. } => {
            cx.interner.is_niche_union(*ty)
                && cx
                    .mir
                    .layouts
                    .union(*ty)
                    .is_none_or(|u| !has_del(cx.mir, &u.name))
        }
        Rvalue::ArrayNew { .. }
        | Rvalue::ArrayLit { .. }
        | Rvalue::ArrayRealloc { .. }
        | Rvalue::Concat(_)
        | Rvalue::ConcatInt { .. }
        | Rvalue::ToString(_)
        | Rvalue::ToBytes { .. }
        | Rvalue::FromBytes { .. }
        | Rvalue::IndirectCall { .. }
        | Rvalue::InterfaceCall { .. }
        | Rvalue::JsCall { .. }
        | Rvalue::FuncRef(_) => false,
    }
}

#[derive(Clone)]
struct WrapSite {
    birth_bi: usize,
    birth_si: usize,
    death_bi: usize,
    death_si: usize,
    /// When true, replace `stmts[death]` with `RegionLeave`. When false, insert leave there.
    replace_death: bool,
    null_locals: Vec<u32>,
}

fn wrap_sites(
    mir: &Mir,
    interner: &TypeInterner,
    fi: usize,
    ctor_only: &HashSet<DefId>,
    memo: &mut HashMap<(DefId, Vec<TypeId>), bool>,
) -> Vec<WrapSite> {
    let f = &mir.functions[fi];
    let mut births: HashMap<u32, (usize, usize, Callee)> = HashMap::new();
    let mut deaths: HashMap<u32, (usize, usize)> = HashMap::new();
    let mut retained = HashSet::new();
    let mut from: HashMap<u32, HashSet<u32>> = HashMap::new();
    let mut extra_death = HashSet::new();
    for (bi, block) in f.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            match stmt {
                Statement::Assign(Place::Local(d), Rvalue::Call { callee, .. }) => {
                    if births.insert(d.0, (bi, si, callee.clone())).is_some() {
                        extra_death.insert(d.0);
                    }
                }
                Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Local(s))))
                    if d.0 != s.0 =>
                {
                    from.entry(d.0).or_default().insert(s.0);
                }
                Statement::Assign(
                    Place::Local(d),
                    Rvalue::Cast(Operand::Copy(Place::Local(s)), _, _),
                ) if d.0 != s.0 => {
                    from.entry(d.0).or_default().insert(s.0);
                }
                Statement::Retain(Operand::Copy(Place::Local(l)))
                | Statement::ValueRetain(l) => {
                    retained.insert(l.0);
                }
                Statement::ReleaseUnique(Operand::Copy(Place::Local(l)))
                | Statement::Release(Operand::Copy(Place::Local(l))) => {
                    if deaths.insert(l.0, (bi, si)).is_some() {
                        extra_death.insert(l.0);
                    }
                    if matches!(stmt, Statement::Release(_)) {
                        extra_death.insert(l.0);
                    }
                }
                Statement::RegionEnter | Statement::RegionLeave => {
                    return Vec::new();
                }
                _ => {}
            }
        }
    }
    let mut visiting = HashSet::new();
    let mut out = Vec::new();
    let mut used_birth = HashSet::new();
    let mut used_death = HashSet::new();
    for (local, (bbi, bsi, callee)) in births {
        let aliases = aliases_of(local, &from);
        if aliases.iter().any(|a| retained.contains(a)) {
            continue;
        }
        let mut cx = SafeCx {
            mir,
            interner,
            ctor_only,
            memo,
            visiting: &mut visiting,
        };
        if !callee_safe(&mut cx, &callee) {
            continue;
        }
        if used_birth.contains(&(bbi, bsi)) {
            continue;
        }
        let alias_deaths: Vec<(usize, usize)> = aliases
            .iter()
            .filter_map(|a| deaths.get(a).copied())
            .collect();
        let unique_ok = alias_deaths.len() == 1
            && !aliases.iter().any(|a| extra_death.contains(a))
            && postdom_death(f, bbi, bsi, alias_deaths[0].0, alias_deaths[0].1);
        if unique_ok {
            let (dbi, dsi) = alias_deaths[0];
            if used_death.contains(&(dbi, dsi)) {
                continue;
            }
            used_birth.insert((bbi, bsi));
            used_death.insert((dbi, dsi));
            out.push(WrapSite {
                birth_bi: bbi,
                birth_si: bsi,
                death_bi: dbi,
                death_si: dsi,
                replace_death: true,
                null_locals: Vec::new(),
            });
            continue;
        }
        if let Some(join) = switch_join(f, bbi, bsi, &aliases) {
            if payload_used_after_join(f, join, bbi, &aliases) {
                continue;
            }
            if used_death.contains(&(join, 0)) {
                continue;
            }
            used_birth.insert((bbi, bsi));
            used_death.insert((join, 0));
            let mut null_locals: Vec<u32> = aliases.iter().copied().collect();
            null_locals.sort_unstable();
            out.push(WrapSite {
                birth_bi: bbi,
                birth_si: bsi,
                death_bi: join,
                death_si: 0,
                replace_death: false,
                null_locals,
            });
        }
    }
    out.sort_by_key(|s| (s.birth_bi, s.birth_si));
    out
}

fn aliases_of(root: u32, from: &HashMap<u32, HashSet<u32>>) -> HashSet<u32> {
    let mut set = HashSet::from([root]);
    let mut changed = true;
    while changed {
        changed = false;
        for (&d, sources) in from {
            if sources.iter().any(|s| set.contains(s)) && set.insert(d) {
                changed = true;
            }
        }
    }
    set
}

fn postdom_death(f: &MirFunction, bbi: usize, bsi: usize, dbi: usize, dsi: usize) -> bool {
    fn dfs(
        f: &MirFunction,
        bi: usize,
        si: usize,
        dbi: usize,
        dsi: usize,
        stack: &mut Vec<(usize, usize)>,
    ) -> bool {
        if stack.contains(&(bi, si)) {
            return false;
        }
        stack.push((bi, si));
        let block = &f.blocks[bi];
        let mut j = si;
        while j < block.stmts.len() {
            if bi == dbi && j == dsi {
                stack.pop();
                return true;
            }
            j += 1;
        }
        let ok = match &block.terminator {
            Terminator::Return(_)
            | Terminator::AsyncComplete(_)
            | Terminator::Unreachable
            | Terminator::TailCall { .. } => false,
            _ => {
                let mut all = true;
                for succ in block.terminator.successors() {
                    if !dfs(f, succ.0 as usize, 0, dbi, dsi, stack) {
                        all = false;
                        break;
                    }
                }
                all
            }
        };
        stack.pop();
        ok
    }
    dfs(f, bbi, bsi + 1, dbi, dsi, &mut Vec::new())
}

fn switch_join(f: &MirFunction, bbi: usize, bsi: usize, aliases: &HashSet<u32>) -> Option<usize> {
    let sbi = find_switch_after(f, bbi, bsi + 1, aliases)?;
    let succs = f.blocks[sbi].terminator.successors();
    if succs.is_empty() {
        return None;
    }
    let mut join: Option<usize> = None;
    for s in succs {
        let j = peel_to_join(f, s.0 as usize)?;
        match join {
            None => join = Some(j),
            Some(x) if x == j => {}
            _ => return None,
        }
    }
    let join = join?;
    if join == bbi || join == sbi {
        return None;
    }
    Some(join)
}

fn payload_used_after_join(
    f: &MirFunction,
    join: usize,
    birth_bi: usize,
    aliases: &HashSet<u32>,
) -> bool {
    let mut seen = HashSet::new();
    let mut stack = vec![join];
    while let Some(bi) = stack.pop() {
        if bi == birth_bi || !seen.insert(bi) {
            continue;
        }
        let Some(block) = f.blocks.get(bi) else {
            continue;
        };
        for stmt in &block.stmts {
            if payload_use_stmt(stmt, aliases) {
                return true;
            }
        }
        if payload_use_term(&block.terminator, aliases) {
            return true;
        }
        for s in block.terminator.successors() {
            stack.push(s.0 as usize);
        }
    }
    false
}

fn payload_use_stmt(stmt: &Statement, aliases: &HashSet<u32>) -> bool {
    match stmt {
        Statement::Retain(_) | Statement::Release(_) | Statement::ReleaseUnique(_) => false,
        Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Const(Const::Null)))
            if aliases.contains(&d.0) =>
        {
            false
        }
        _ => aliases
            .iter()
            .any(|a| crate::passes::rc::stmt_reads_local(stmt, *a)),
    }
}

fn payload_use_term(term: &Terminator, aliases: &HashSet<u32>) -> bool {
    match term {
        Terminator::Return(Some(o)) | Terminator::AsyncComplete(Some(o)) => {
            operand_alias(o, aliases)
        }
        Terminator::If { cond, .. } => operand_alias(cond, aliases),
        Terminator::Switch { value, .. } => operand_alias(value, aliases),
        Terminator::TailCall { args, .. } => args.iter().any(|a| operand_alias(a, aliases)),
        Terminator::Await { future, .. } => operand_alias(future, aliases),
        _ => false,
    }
}

fn find_switch_after(
    f: &MirFunction,
    mut bi: usize,
    mut si: usize,
    aliases: &HashSet<u32>,
) -> Option<usize> {
    let mut seen = HashSet::new();
    let mut keys = aliases.clone();
    loop {
        if !seen.insert(bi) {
            return None;
        }
        let block = f.blocks.get(bi)?;
        for stmt in block.stmts.iter().skip(si) {
            if let Statement::Assign(Place::Local(d), rv) = stmt {
                if disc_of_alias(rv, &keys) {
                    keys.insert(d.0);
                }
            }
        }
        match &block.terminator {
            Terminator::Switch { value, .. } if operand_alias(value, &keys) => {
                return Some(bi);
            }
            Terminator::If { cond, .. } if operand_alias(cond, &keys) => {
                return Some(bi);
            }
            Terminator::Goto(b) => {
                bi = b.0 as usize;
                si = 0;
            }
            _ => return None,
        }
    }
}

fn disc_of_alias(rv: &Rvalue, keys: &HashSet<u32>) -> bool {
    match rv {
        Rvalue::Discriminant { base, .. } => operand_alias(base, keys),
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            operand_alias(cond, keys)
                || operand_alias(then_val, keys)
                || operand_alias(else_val, keys)
        }
        Rvalue::Binary(_, a, b) => operand_alias(a, keys) || operand_alias(b, keys),
        Rvalue::Use(o) | Rvalue::Cast(o, _, _) | Rvalue::IsType(o, _) => operand_alias(o, keys),
        _ => false,
    }
}

fn operand_alias(op: &Operand, aliases: &HashSet<u32>) -> bool {
    matches!(op, Operand::Copy(Place::Local(l)) if aliases.contains(&l.0))
}

fn peel_to_join(f: &MirFunction, bi: usize) -> Option<usize> {
    let block = f.blocks.get(bi)?;
    for stmt in &block.stmts {
        if !join_arm_ok(stmt) {
            return None;
        }
    }
    match &block.terminator {
        Terminator::Goto(j) => Some(j.0 as usize),
        _ => None,
    }
}

fn join_arm_ok(stmt: &Statement) -> bool {
    match stmt {
        Statement::Nop
        | Statement::DebugLine(_)
        | Statement::SourceLine(_)
        | Statement::Retain(_)
        | Statement::Release(_)
        | Statement::ReleaseUnique(_) => true,
        Statement::Assign(Place::Local(_), rv) => !matches!(
            rv,
            Rvalue::Call { .. }
                | Rvalue::New { .. }
                | Rvalue::ArrayNew { .. }
                | Rvalue::IndirectCall { .. }
                | Rvalue::InterfaceCall { .. }
                | Rvalue::JsCall { .. }
        ),
        _ => false,
    }
}

fn apply_wraps(f: &mut MirFunction, sites: &[WrapSite]) {
    let mut sites = sites.to_vec();
    sites.sort_by_key(|s| (std::cmp::Reverse(s.birth_bi), std::cmp::Reverse(s.birth_si)));
    for site in sites {
        let mut death_si = site.death_si;
        f.blocks[site.birth_bi]
            .stmts
            .insert(site.birth_si, Statement::RegionEnter);
        if site.death_bi == site.birth_bi && death_si >= site.birth_si {
            death_si += 1;
        }
        let mut leave = vec![Statement::RegionLeave];
        for loc in &site.null_locals {
            leave.push(Statement::Assign(
                Place::Local(Local(*loc)),
                Rvalue::Use(Operand::Const(Const::Null)),
            ));
        }
        if site.replace_death {
            f.blocks[site.death_bi].stmts[death_si] = Statement::RegionLeave;
        } else {
            let block = &mut f.blocks[site.death_bi];
            let tail = block.stmts.split_off(death_si);
            block.stmts.extend(leave);
            block.stmts.extend(tail);
        }
    }
}

fn strip_escaped_fn(f: &mut MirFunction, interner: &TypeInterner) -> bool {
    let mut stack = Vec::new();
    let mut drop_at: HashSet<(usize, usize)> = HashSet::new();
    for (bi, block) in f.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            match stmt {
                Statement::RegionEnter => stack.push((bi, si)),
                Statement::RegionLeave => {
                    if let Some(enter) = stack.pop() {
                        if rc_use_after_leave(f, interner, bi, si) {
                            drop_at.insert(enter);
                            drop_at.insert((bi, si));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if drop_at.is_empty() {
        return false;
    }
    for (bi, block) in f.blocks.iter_mut().enumerate() {
        let mut si = block.stmts.len();
        while si > 0 {
            si -= 1;
            if drop_at.contains(&(bi, si)) {
                block.stmts.remove(si);
            }
        }
    }
    true
}

fn rc_use_after_leave(
    f: &MirFunction,
    interner: &TypeInterner,
    leave_bi: usize,
    leave_si: usize,
) -> bool {
    let mut seen = HashSet::from([leave_bi]);
    let mut stack = vec![(leave_bi, leave_si + 1, HashSet::new())];
    while let Some((bi, si0, mut killed)) = stack.pop() {
        if si0 == 0 && !seen.insert(bi) {
            continue;
        }
        let Some(block) = f.blocks.get(bi) else {
            continue;
        };
        for stmt in block.stmts.iter().skip(si0) {
            if rc_stmt_escapes(f, interner, stmt, &killed) {
                return true;
            }
            if let Statement::Assign(Place::Local(d), _) = stmt {
                killed.insert(d.0);
            }
        }
        if rc_term_escapes(f, interner, &block.terminator, &killed) {
            return true;
        }
        for s in block.terminator.successors() {
            let nbi = s.0 as usize;
            if nbi == leave_bi {
                continue;
            }
            stack.push((nbi, 0, killed.clone()));
        }
    }
    false
}

fn rc_stmt_escapes(
    f: &MirFunction,
    interner: &TypeInterner,
    stmt: &Statement,
    killed: &HashSet<u32>,
) -> bool {
    match stmt {
        Statement::Retain(_) | Statement::Release(_) | Statement::ReleaseUnique(_) => false,
        Statement::Assign(Place::Local(_), Rvalue::Use(Operand::Const(Const::Null))) => false,
        Statement::RegionEnter | Statement::RegionLeave => false,
        _ => f.locals.iter().enumerate().any(|(i, loc)| {
            let i = i as u32;
            !killed.contains(&i)
                && interner.is_rc_tracked(loc.ty)
                && crate::passes::rc::stmt_reads_local(stmt, i)
        }),
    }
}

fn rc_term_escapes(
    f: &MirFunction,
    interner: &TypeInterner,
    term: &Terminator,
    killed: &HashSet<u32>,
) -> bool {
    let mut live = HashSet::new();
    match term {
        Terminator::Return(Some(o)) | Terminator::AsyncComplete(Some(o)) => {
            operand_locals(o, &mut live);
        }
        Terminator::If { cond, .. } => operand_locals(cond, &mut live),
        Terminator::Switch { value, .. } => operand_locals(value, &mut live),
        Terminator::TailCall { args, .. } => {
            for a in args {
                operand_locals(a, &mut live);
            }
        }
        Terminator::Await { future, .. } => operand_locals(future, &mut live),
        _ => {}
    }
    live.iter().any(|i| {
        !killed.contains(i)
            && f.locals
                .get(*i as usize)
                .is_some_and(|loc| interner.is_rc_tracked(loc.ty))
    })
}

fn operand_locals(op: &Operand, live: &mut HashSet<u32>) {
    if let Operand::Copy(place) = op {
        match place {
            Place::Local(l) => {
                live.insert(l.0);
            }
            Place::Field { base, .. } | Place::Deref { ptr: base, .. } => {
                live.insert(base.0);
            }
            Place::Index { base, index, .. } => {
                live.insert(base.0);
                operand_locals(index, live);
            }
            Place::Global(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::passes::ModulePass;
    use dream_hir::{LayoutTable, TypeLayout};
    use dream_types::{DefKind, TypeCtx};

    #[test]
    fn wraps_unique_call_that_only_news_del_free_class() {
        let mut ctx = TypeCtx::new();
        let node_def = ctx.register(DefKind::Struct, "Node", vec![]);
        let ty = ctx.interner.struct_ty(node_def, vec![]);
        let alloc_def = ctx.register(DefKind::Function, "alloc_node", vec![]);
        let drop_def = ctx.register(DefKind::Function, "drop_it", vec![]);
        let layout = TypeLayout::from_fields(&ctx.interner, "Node", vec![]);
        let mut layouts = LayoutTable::default();
        layouts.insert(ty, layout);

        let mut alloc = FunctionBuilder::new("alloc_node", ty);
        alloc.set_def(alloc_def, vec![]);
        let t = alloc.new_local(ty, Some("t".into()));
        alloc.assign(
            Place::Local(t),
            Rvalue::New {
                def: node_def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        alloc.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

        let mut drop_it = FunctionBuilder::new("drop_it", ctx.interner.void());
        drop_it.set_def(drop_def, vec![]);
        let x = drop_it.new_local(ty, Some("x".into()));
        drop_it.assign(
            Place::Local(x),
            Rvalue::Call {
                callee: Callee {
                    def: alloc_def,
                    args: vec![],
                    ret: ty,
                    take_params: vec![],
                },
                args: vec![],
            },
        );
        drop_it.push(Statement::ReleaseUnique(Operand::Copy(Place::Local(x))));
        drop_it.terminate(Terminator::Return(None));

        let mut mir = Mir {
            functions: vec![alloc.finish(), drop_it.finish()],
            layouts,
            ..Default::default()
        };
        assert!(UniqueRegion.run(&mut mir, &ctx.interner));
        let drop_fn = &mir.functions[1];
        assert!(
            matches!(drop_fn.blocks[0].stmts[0], Statement::RegionEnter),
            "{:?}",
            drop_fn.blocks[0].stmts
        );
        assert!(
            drop_fn.blocks[0]
                .stmts
                .iter()
                .any(|s| matches!(s, Statement::RegionLeave)),
            "{:?}",
            drop_fn.blocks[0].stmts
        );
        assert!(!drop_fn.blocks[0]
            .stmts
            .iter()
            .any(|s| matches!(s, Statement::ReleaseUnique(_))),);
        let c = crate::backend::c::emit_c_module(&mir, &ctx.interner);
        assert!(
            c.contains("dream_region_enter") && c.contains("dream_region_leave"),
            "{}",
            c
        );
    }

    #[test]
    fn wraps_switch_join_of_unique_call() {
        use crate::{BinOp, BlockId};

        let mut ctx = TypeCtx::new();
        let node_def = ctx.register(DefKind::Struct, "Node", vec![]);
        let ty = ctx.interner.struct_ty(node_def, vec![]);
        let alloc_def = ctx.register(DefKind::Function, "alloc_node", vec![]);
        let drop_def = ctx.register(DefKind::Function, "drop_it", vec![]);
        let layout = TypeLayout::from_fields(&ctx.interner, "Node", vec![]);
        let mut layouts = LayoutTable::default();
        layouts.insert(ty, layout);

        let mut alloc = FunctionBuilder::new("alloc_node", ty);
        alloc.set_def(alloc_def, vec![]);
        let t = alloc.new_local(ty, Some("t".into()));
        alloc.assign(
            Place::Local(t),
            Rvalue::New {
                def: node_def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        alloc.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

        let mut drop_it = FunctionBuilder::new("drop_it", ctx.interner.void());
        drop_it.set_def(drop_def, vec![]);
        let x = drop_it.new_local(ty, Some("x".into()));
        let acc = drop_it.new_local(ctx.interner.int(), Some("acc".into()));
        drop_it.assign(
            Place::Local(x),
            Rvalue::Call {
                callee: Callee {
                    def: alloc_def,
                    args: vec![],
                    ret: ty,
                    take_params: vec![],
                },
                args: vec![],
            },
        );
        let some = drop_it.new_block();
        let none = drop_it.new_block();
        let join = drop_it.new_block();
        drop_it.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(x)),
            then_blk: some,
            else_blk: none,
        });
        drop_it.switch_to(some);
        drop_it.assign(
            Place::Local(acc),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(acc)),
                Operand::Const(Const::Int(1)),
            ),
        );
        drop_it.terminate(Terminator::Goto(join));
        drop_it.switch_to(none);
        drop_it.terminate(Terminator::Goto(join));
        drop_it.switch_to(join);
        drop_it.terminate(Terminator::Return(None));

        let mut mir = Mir {
            functions: vec![alloc.finish(), drop_it.finish()],
            layouts,
            ..Default::default()
        };
        assert!(UniqueRegion.run(&mut mir, &ctx.interner));
        let drop_fn = &mir.functions[1];
        assert!(
            matches!(drop_fn.blocks[0].stmts[0], Statement::RegionEnter),
            "{:?}",
            drop_fn.blocks[0].stmts
        );
        let join_id = BlockId(3);
        assert!(
            matches!(drop_fn.block(join_id).stmts[0], Statement::RegionLeave),
            "{:?}",
            drop_fn.block(join_id).stmts
        );
    }

    #[test]
    fn does_not_wrap_switch_join_when_phi_used_after() {
        let mut ctx = TypeCtx::new();
        let node_def = ctx.register(DefKind::Struct, "Node", vec![]);
        let ty = ctx.interner.struct_ty(node_def, vec![]);
        let alloc_def = ctx.register(DefKind::Function, "alloc_node", vec![]);
        let drop_def = ctx.register(DefKind::Function, "drop_it", vec![]);
        let layout = TypeLayout::from_fields(&ctx.interner, "Node", vec![]);
        let mut layouts = LayoutTable::default();
        layouts.insert(ty, layout);

        let mut alloc = FunctionBuilder::new("alloc_node", ty);
        alloc.set_def(alloc_def, vec![]);
        let t = alloc.new_local(ty, Some("t".into()));
        alloc.assign(
            Place::Local(t),
            Rvalue::New {
                def: node_def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        alloc.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

        let mut drop_it = FunctionBuilder::new("drop_it", ctx.interner.void());
        drop_it.set_def(drop_def, vec![]);
        let x = drop_it.new_local(ty, Some("x".into()));
        let other = drop_it.new_local(ty, Some("other".into()));
        let phi = drop_it.new_local(ty, Some("phi".into()));
        let s = drop_it.new_local(ctx.interner.string(), Some("s".into()));
        drop_it.assign(
            Place::Local(x),
            Rvalue::Call {
                callee: Callee {
                    def: alloc_def,
                    args: vec![],
                    ret: ty,
                    take_params: vec![],
                },
                args: vec![],
            },
        );
        let some = drop_it.new_block();
        let none = drop_it.new_block();
        let join = drop_it.new_block();
        drop_it.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(x)),
            then_blk: some,
            else_blk: none,
        });
        drop_it.switch_to(some);
        drop_it.assign(
            Place::Local(phi),
            Rvalue::Use(Operand::Copy(Place::Local(x))),
        );
        drop_it.terminate(Terminator::Goto(join));
        drop_it.switch_to(none);
        drop_it.assign(
            Place::Local(phi),
            Rvalue::Use(Operand::Copy(Place::Local(other))),
        );
        drop_it.terminate(Terminator::Goto(join));
        drop_it.switch_to(join);
        drop_it.assign(
            Place::Local(s),
            Rvalue::ToString(Operand::Copy(Place::Local(phi))),
        );
        drop_it.terminate(Terminator::Return(None));

        let mut mir = Mir {
            functions: vec![alloc.finish(), drop_it.finish()],
            layouts,
            ..Default::default()
        };
        UniqueRegion.run(&mut mir, &ctx.interner);
        let drop_fn = &mir.functions[1];
        assert!(
            !drop_fn
                .blocks
                .iter()
                .any(|b| b.stmts.iter().any(|s| matches!(s, Statement::RegionEnter))),
            "{:?}",
            drop_fn.blocks
        );
    }

    #[test]
    fn strip_escaped_drops_leave_before_payload_use() {
        let mut ctx = TypeCtx::new();
        let node_def = ctx.register(DefKind::Struct, "Node", vec![]);
        let ty = ctx.interner.struct_ty(node_def, vec![]);
        let alloc_def = ctx.register(DefKind::Function, "alloc_node", vec![]);
        let drop_def = ctx.register(DefKind::Function, "drop_it", vec![]);
        let layout = TypeLayout::from_fields(&ctx.interner, "Node", vec![]);
        let mut layouts = LayoutTable::default();
        layouts.insert(ty, layout);

        let mut alloc = FunctionBuilder::new("alloc_node", ty);
        alloc.set_def(alloc_def, vec![]);
        let t = alloc.new_local(ty, Some("t".into()));
        alloc.assign(
            Place::Local(t),
            Rvalue::New {
                def: node_def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        alloc.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

        let mut drop_it = FunctionBuilder::new("drop_it", ctx.interner.void());
        drop_it.set_def(drop_def, vec![]);
        let x = drop_it.new_local(ty, Some("x".into()));
        let s = drop_it.new_local(ctx.interner.string(), Some("s".into()));
        drop_it.push(Statement::RegionEnter);
        drop_it.assign(
            Place::Local(x),
            Rvalue::Call {
                callee: Callee {
                    def: alloc_def,
                    args: vec![],
                    ret: ty,
                    take_params: vec![],
                },
                args: vec![],
            },
        );
        drop_it.push(Statement::RegionLeave);
        drop_it.assign(
            Place::Local(s),
            Rvalue::ToString(Operand::Copy(Place::Local(x))),
        );
        drop_it.terminate(Terminator::Return(None));

        let mut mir = Mir {
            functions: vec![alloc.finish(), drop_it.finish()],
            layouts,
            ..Default::default()
        };
        assert!(strip_escaped_regions(&mut mir, &ctx.interner));
        let drop_fn = &mir.functions[1];
        assert!(
            !drop_fn
                .blocks
                .iter()
                .any(|b| b
                    .stmts
                    .iter()
                    .any(|s| matches!(s, Statement::RegionEnter | Statement::RegionLeave))),
            "{:?}",
            drop_fn.blocks[0].stmts
        );
    }
}
