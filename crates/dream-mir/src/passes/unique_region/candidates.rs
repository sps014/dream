use super::*;

pub(super) fn ctor_only_defs(mir: &Mir) -> IndexSet<DefId> {
    let mut as_ctor = IndexSet::new();
    let mut as_call = IndexSet::new();
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
                    as_ctor.insert(ctor.def);
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

pub(super) fn walk_fn(f: &MirFunction, mut visit: impl FnMut(&Statement)) {
    for b in &f.blocks {
        for s in &b.stmts {
            visit(s);
        }
    }
}

#[derive(Clone)]
pub(super) struct WrapSite {
    pub(super) birth_bi: usize,
    pub(super) birth_si: usize,
    pub(super) death_bi: usize,
    pub(super) death_si: usize,
    /// When true, replace `stmts[death]` with `RegionLeave`. When false, insert leave there.
    pub(super) replace_death: bool,
    pub(super) null_locals: Vec<u32>,
}

pub(super) fn wrap_sites(
    mir: &Mir,
    interner: &TypeInterner,
    fi: usize,
    ctor_only: &IndexSet<DefId>,
    memo: &IndexMap<(DefId, Vec<TypeId>), bool>,
) -> Vec<WrapSite> {
    let f = &mir.functions[fi];
    let mut births: BTreeMap<u32, (usize, usize, Callee)> = BTreeMap::new();
    let mut deaths: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    let mut retained = BTreeSet::new();
    let mut from: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    let mut extra_death = BTreeSet::new();
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
                Statement::Retain(Operand::Copy(Place::Local(l))) | Statement::ValueRetain(l) => {
                    retained.insert(l.0);
                }
                Statement::Release(Operand::Copy(Place::Local(l))) => {
                    if deaths.insert(l.0, (bi, si)).is_some() {
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
    let mut out = Vec::new();
    let mut used_birth = BTreeSet::new();
    let mut used_death = BTreeSet::new();
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
            && unobserved_root(f, &aliases, (bbi, bsi))
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
            let null_locals: Vec<u32> = aliases.iter().copied().collect();
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

fn unobserved_root(f: &MirFunction, aliases: &BTreeSet<u32>, birth: (usize, usize)) -> bool {
    // A plain Release proves no uniqueness. Only eliminate its walk when the fresh result
    // stayed entirely in local slots: calls and field/payload access can introduce hidden owners.
    f.blocks.iter().enumerate().all(|(bi, block)| {
        block.stmts.iter().enumerate().all(|(si, stmt)| match stmt {
            _ if (bi, si) == birth => !aliases
                .iter()
                .any(|a| crate::passes::rc::stmt_reads_local(stmt, *a)),
            Statement::Assign(Place::Local(d), rv) if aliases.contains(&d.0) => match rv {
                Rvalue::Use(Operand::Const(Const::Null)) => true,
                Rvalue::Use(Operand::Copy(Place::Local(s)))
                | Rvalue::Cast(Operand::Copy(Place::Local(s)), _, _) => aliases.contains(&s.0),
                _ => false,
            },
            Statement::Assign(
                Place::Local(_),
                Rvalue::Use(Operand::Copy(Place::Local(_)))
                | Rvalue::Cast(Operand::Copy(Place::Local(_)), _, _),
            )
            | Statement::Release(_) => true,
            _ => !aliases
                .iter()
                .any(|a| crate::passes::rc::stmt_reads_local(stmt, *a)),
        }) && !payload_use_term(&block.terminator, aliases)
    })
}

pub(super) fn aliases_of(root: u32, from: &BTreeMap<u32, BTreeSet<u32>>) -> BTreeSet<u32> {
    let mut set = BTreeSet::from([root]);
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

pub(super) fn postdom_death(
    f: &MirFunction,
    bbi: usize,
    bsi: usize,
    dbi: usize,
    dsi: usize,
) -> bool {
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

pub(super) fn switch_join(
    f: &MirFunction,
    bbi: usize,
    bsi: usize,
    aliases: &BTreeSet<u32>,
) -> Option<usize> {
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

pub(super) fn payload_used_after_join(
    f: &MirFunction,
    join: usize,
    birth_bi: usize,
    aliases: &BTreeSet<u32>,
) -> bool {
    let mut seen = IndexSet::new();
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

pub(super) fn payload_use_stmt(stmt: &Statement, aliases: &BTreeSet<u32>) -> bool {
    match stmt {
        Statement::Retain(_) | Statement::Release(_) => false,
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

pub(super) fn payload_use_term(term: &Terminator, aliases: &BTreeSet<u32>) -> bool {
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

pub(super) fn find_switch_after(
    f: &MirFunction,
    mut bi: usize,
    mut si: usize,
    aliases: &BTreeSet<u32>,
) -> Option<usize> {
    let mut seen = IndexSet::new();
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

pub(super) fn disc_of_alias(rv: &Rvalue, keys: &BTreeSet<u32>) -> bool {
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

pub(super) fn operand_alias(op: &Operand, aliases: &BTreeSet<u32>) -> bool {
    matches!(op, Operand::Copy(Place::Local(l)) if aliases.contains(&l.0))
}

pub(super) fn peel_to_join(f: &MirFunction, bi: usize) -> Option<usize> {
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

pub(super) fn join_arm_ok(stmt: &Statement) -> bool {
    match stmt {
        Statement::Nop
        | Statement::DebugLine(_)
        | Statement::SourceLine(_)
        | Statement::Retain(_)
        | Statement::Release(_) => true,
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
