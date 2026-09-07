//! Cursor inference: mark non-escaping field/index loads as non-owning aliases.

use super::liveness::{self, live_after_stmt, stmt_reads_local};
use crate::{Callee, Const, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::LayoutTable;
use dream_types::TypeInterner;
use std::collections::HashSet;

/// Mark locals that only hold a non-escaping field/index (or union-field) load, or a forwarding
/// copy of another RC local, as cursors so [`super::RcInsertion`] skips retain/release on them.
pub(crate) fn infer_cursors(func: &mut MirFunction, interner: &TypeInterner, layouts: &LayoutTable) {
    let n = func.locals.len();
    let params: HashSet<u32> = func.params.iter().map(|p| p.0).collect();
    let mut candidates: HashSet<u32> = HashSet::new();
    let mut forwarding: HashSet<u32> = HashSet::new();
    let mut forwarding_copies: Vec<(u32, u32, usize, usize)> = Vec::new();
    let mut escaped: HashSet<u32> = HashSet::new();
    let mut def_count: Vec<u32> = vec![0; n];
    let mut index_defined: HashSet<u32> = HashSet::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
                if matches!(
                    rvalue,
                    Rvalue::Use(Operand::Copy(Place::Index { .. }))
                        | Rvalue::Cast(Operand::Copy(Place::Index { .. }), _, _)
                ) {
                    index_defined.insert(dest.0);
                }
            }
        }
    }

    for (bi, block) in func.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
                let d = dest.0 as usize;
                if d < n && !is_null_init(rvalue) {
                    def_count[d] += 1;
                }
                let rc = !params.contains(&dest.0)
                    && interner.is_rc_tracked(func.locals[dest.0 as usize].ty);
                if rc && is_cursor_source(rvalue) {
                    candidates.insert(dest.0);
                } else if rc && is_forwarding_copy(rvalue) {
                    if let Rvalue::Use(Operand::Copy(Place::Local(src))) = rvalue {
                        if func.locals[src.0 as usize].ty == func.locals[dest.0 as usize].ty {
                            forwarding.insert(dest.0);
                            forwarding_copies.push((dest.0, src.0, bi, si));
                        } else {
                            escaped.insert(dest.0);
                            forwarding.remove(&dest.0);
                        }
                    }
                } else if is_null_init(rvalue) {
                    // Lowering null-inits every local (`x = null; x = this.f`). That is not a
                    // second owner and must not escape a later field/index snapshot: leftover
                    // last-ref of an owned `this.obj_map` copy frees the Map still in `JsonValue`.
                } else if !is_cursor_source(rvalue) && !is_forwarding_copy(rvalue) {
                    escaped.insert(dest.0);
                    forwarding.remove(&dest.0);
                }
            }
            mark_stmt_escapes(stmt, &mut escaped);
        }
        mark_term_escapes(&block.terminator, &mut escaped);
    }

    // Occupants of arrays/maps: Field of an index load (and UnionField of that Option)
    // must own. Cursor-walking through `this.obj_map` would leftover-last-ref the value
    // still stored in the Map (`JsonValue.get` / `unwrap_or`).
    {
        let mut snapshot_of: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        for block in &func.blocks {
            for stmt in &block.stmts {
                if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
                    if let Some(base) = snapshot_base(rvalue) {
                        snapshot_of.insert(dest.0, base);
                    }
                }
            }
        }
        let mut copy_of: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        for &(dest, src, _, _) in &forwarding_copies {
            copy_of.insert(dest, src);
        }
        let reaches_index = |mut x: u32| {
            let mut seen = HashSet::new();
            while seen.insert(x) {
                if index_defined.contains(&x) {
                    return true;
                }
                if let Some(&s) = copy_of.get(&x) {
                    x = s;
                } else if let Some(&s) = snapshot_of.get(&x) {
                    x = s;
                } else {
                    break;
                }
            }
            index_defined.contains(&x)
        };
        for block in &func.blocks {
            for stmt in &block.stmts {
                if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
                    let Some(base) = snapshot_base(rvalue) else {
                        continue;
                    };
                    if reaches_index(base) {
                        candidates.remove(&dest.0);
                        escaped.insert(dest.0);
                        forwarding.remove(&dest.0);
                    }
                }
            }
        }
    }

    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Statement::Assign(
                Place::Local(dest),
                Rvalue::Use(Operand::Copy(Place::Local(src))),
            ) = stmt
            {
                if !forwarding.contains(&dest.0) {
                    escaped.insert(src.0);
                }
            }
        }
    }

    let mut overwrite_escaped: HashSet<u32> = HashSet::new();
    escape_slot_overwrite_readers(func, &mut candidates, &mut escaped, &mut overwrite_escaped);
    let mut outlive_escaped: HashSet<u32> = HashSet::new();
    escape_cursors_outliving_base(
        func,
        &mut candidates,
        &mut escaped,
        &mut outlive_escaped,
        &forwarding_copies,
    );

    for (i, &defs) in def_count.iter().enumerate() {
        let id = i as u32;
        if defs != 1 || params.contains(&id) || func.locals[i].is_take {
            escaped.insert(id);
        }
    }

    // Codegen materializes `Await.dest` in the resume block. Forwarding copies of that
    // dest (`let g = await …`) must own the result or the unique RC from `AsyncComplete`
    // is never released (async_rc_return live_delta).
    for block in &func.blocks {
        if let Terminator::Await {
            dest: Some(d),
            resume,
            ..
        } = &block.terminator
        {
            escaped.insert(d.0);
            forwarding.remove(&d.0);
            let ri = resume.0 as usize;
            if ri < func.blocks.len() {
                for stmt in &func.blocks[ri].stmts {
                    if let Statement::Assign(
                        Place::Local(user),
                        Rvalue::Use(Operand::Copy(Place::Local(src))),
                    ) = stmt
                    {
                        if src.0 == d.0 {
                            escaped.insert(user.0);
                            forwarding.remove(&user.0);
                        }
                    }
                }
            }
        }
    }

    for &id in &candidates {
        if !escaped.contains(&id) {
            func.locals[id as usize].is_cursor = true;
        }
    }

    // `this.f` of a borrow/`this` param (or of a cursor into one) is an alias of a slot the
    // callee does not own. Must run before forwarding copies so `m = this_2.obj_map` stays a
    // cursor (`JsonValue.get` leftover last-ref would free the Map).
    //
    // Do not walk through index loads: `slots[i].value` is a Map occupant. Treating it as a
    // cursor lets leftover last-ref of `JsonValue.get` / `unwrap_or` destroy the value still
    // stored in `obj_map`.
    let borrow_params: HashSet<u32> = func
        .params
        .iter()
        .filter(|p| !func.locals[p.0 as usize].is_take)
        .map(|p| p.0)
        .collect();
    let mut grew = true;
    while grew {
        grew = false;
        for block in &func.blocks {
            for stmt in &block.stmts {
                let Statement::Assign(Place::Local(dest), rvalue) = stmt else {
                    continue;
                };
                if overwrite_escaped.contains(&dest.0)
                    || outlive_escaped.contains(&dest.0)
                    || func.locals[dest.0 as usize].is_cursor
                {
                    continue;
                }
                if !borrow_field_snapshot(rvalue, &func.locals, &borrow_params, &index_defined) {
                    continue;
                }
                func.locals[dest.0 as usize].is_cursor = true;
                grew = true;
            }
        }
    }

    // A `weak`/`unowned` field load is not a strong owner. Retaining it would keep the
    // referent alive after the last strong leftover (`weak_field_runtime`).
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(dest), rvalue) = stmt else {
                continue;
            };
            // Slot overwrite must not force a strong owner: a weak store does not last-ref
            // the occupant, and the following load is still a non-owning alias.
            if !weak_or_unowned_field_load(rvalue, func, layouts) {
                continue;
            }
            func.locals[dest.0 as usize].is_cursor = true;
        }
    }

    let live_out = liveness::live_out(func);
    let mut grew = true;
    while grew {
        grew = false;
        for &(dest, src, bi, si) in &forwarding_copies {
            if !forwarding.contains(&dest)
                || escaped.contains(&dest)
                || overwrite_escaped.contains(&dest)
                || outlive_escaped.contains(&dest)
                || func.locals[dest as usize].is_cursor
            {
                continue;
            }
            // Last-use `y = x` is a move (dest owns). A still-live source is an alias cursor
            // (`t = s; return s`). Copies of field snapshots stay cursors.
            if func.locals[src as usize].is_cursor
                || live_after_stmt(func, &live_out, bi, si, src)
            {
                func.locals[dest as usize].is_cursor = true;
                grew = true;
            }
        }
    }
}

fn snapshot_base(rvalue: &Rvalue) -> Option<u32> {
    match cursor_source_slot(rvalue) {
        Some(SourceSlot::Field(b, _) | SourceSlot::IndexBase(b) | SourceSlot::UnionBase(b)) => {
            Some(b)
        }
        None => None,
    }
}

/// Field / UnionField of borrow `this` (and Field of a cursor Map in that object). Index loads
/// and field snapshots of those loads own a retain — they alias container occupants.
fn weak_or_unowned_field_load(
    rvalue: &Rvalue,
    func: &MirFunction,
    layouts: &LayoutTable,
) -> bool {
    let (base, field) = match rvalue {
        Rvalue::Use(Operand::Copy(Place::Field { base, field }))
        | Rvalue::Cast(Operand::Copy(Place::Field { base, field }), _, _) => (*base, *field),
        _ => return false,
    };
    layouts
        .get(func.local_ty(base))
        .and_then(|layout| layout.fields.get(field))
        .is_some_and(|f| f.is_weak || f.is_unowned)
}

fn borrow_field_snapshot(
    rvalue: &Rvalue,
    locals: &[crate::LocalDecl],
    borrow_params: &HashSet<u32>,
    index_defined: &HashSet<u32>,
) -> bool {
    let Some(slot) = cursor_source_slot(rvalue) else {
        return false;
    };
    match slot {
        SourceSlot::IndexBase(_) => false,
        SourceSlot::Field(base, _) => {
            !index_defined.contains(&base)
                && (borrow_params.contains(&base) || locals[base as usize].is_cursor)
        }
        SourceSlot::UnionBase(base) => {
            !index_defined.contains(&base)
                && (borrow_params.contains(&base) || locals[base as usize].is_cursor)
        }
    }
}

fn is_null_init(rvalue: &Rvalue) -> bool {
    matches!(rvalue, Rvalue::Use(Operand::Const(Const::Null)))
}

fn is_forwarding_copy(rvalue: &Rvalue) -> bool {
    matches!(rvalue, Rvalue::Use(Operand::Copy(Place::Local(_))))
}

fn is_cursor_source(rvalue: &Rvalue) -> bool {
    matches!(
        rvalue,
        Rvalue::Use(Operand::Copy(Place::Field { .. }))
            | Rvalue::UnionField { .. }
            | Rvalue::Cast(Operand::Copy(Place::Field { .. }), _, _)
    )
}

/// Where a cursor-source rvalue reads from, at slot granularity. A cursor is sound only while
/// its source slot keeps holding the object; any store to that slot (or wholesale overwrite of
/// the base) releases the previous occupant, so readers of stored-to slots cannot stay cursors.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum SourceSlot {
    Field(u32, u32),
    IndexBase(u32),
    UnionBase(u32),
}

fn cursor_source_slot(rvalue: &Rvalue) -> Option<SourceSlot> {
    let field_or_index = |op: &Operand| match op {
        Operand::Copy(Place::Field { base, field }) => {
            Some(SourceSlot::Field(base.0, *field as u32))
        }
        Operand::Copy(Place::Index { base, .. }) => Some(SourceSlot::IndexBase(base.0)),
        _ => None,
    };
    match rvalue {
        Rvalue::Use(op) | Rvalue::Cast(op, _, _) => field_or_index(op),
        Rvalue::UnionField { base, .. } => {
            let Operand::Copy(Place::Local(l)) = base else {
                return None;
            };
            Some(SourceSlot::UnionBase(l.0))
        }
        _ => None,
    }
}

/// Escape candidacy for candidates whose source slot is overwritten anywhere in the function.
/// Flow-insensitive on purpose: over-approximating only converts cursors to owners, which can
/// never under-retain; missing an overwrite would leave the use-after-free this guards against.
fn escape_slot_overwrite_readers(
    func: &MirFunction,
    candidates: &mut HashSet<u32>,
    escaped: &mut HashSet<u32>,
    overwrite_escaped: &mut HashSet<u32>,
) {
    let mut stored_fields: HashSet<(u32, u32)> = HashSet::new();
    let mut stored_index_bases: HashSet<u32> = HashSet::new();
    let mut def_counts: Vec<u32> = vec![0; func.locals.len()];
    for block in &func.blocks {
        for stmt in &block.stmts {
            match stmt {
                Statement::Assign(Place::Field { base, field }, _) => {
                    stored_fields.insert((base.0, *field as u32));
                }
                Statement::Assign(Place::Index { base, .. }, _) => {
                    stored_index_bases.insert(base.0);
                }
                Statement::Assign(Place::Local(l), rv) if !is_null_init(rv) => {
                    def_counts[l.0 as usize] += 1;
                }
                _ => {}
            }
        }
    }
    // A base's *initializing* assignment creates the object; only a re-definition (2nd+ def)
    // can drop a previous occupant and invalidate snapshots taken through the base.
    let overwritten_bases: HashSet<u32> = def_counts
        .iter()
        .enumerate()
        .filter(|(_, &n)| n > 1)
        .map(|(i, _)| i as u32)
        .collect();

    let sources: Vec<(u32, SourceSlot)> = func
        .blocks
        .iter()
        .flat_map(|block| &block.stmts)
        .filter_map(|stmt| match stmt {
            Statement::Assign(Place::Local(dest), rvalue) if candidates.contains(&dest.0) => {
                cursor_source_slot(rvalue).map(|slot| (dest.0, slot))
            }
            _ => None,
        })
        .collect();

    for (id, slot) in sources {
        let overwritten = match slot {
            SourceSlot::Field(base, field) => {
                stored_fields.contains(&(base, field)) || overwritten_bases.contains(&base)
            }
            SourceSlot::IndexBase(base) => {
                // Loads and stores often materialize different temps for the same array
                // (`t0 = arr; x = t0[i]` vs `t1 = arr; t1[i] = w`). Matching only the load's
                // base would leave `x` a cursor across the store.
                stored_index_bases.contains(&base)
                    || overwritten_bases.contains(&base)
                    || !stored_index_bases.is_empty()
            }
            // A union local's whole value is its slot: any rebind frees the old payload.
            SourceSlot::UnionBase(base) => overwritten_bases.contains(&base),
        };
        if overwritten {
            candidates.remove(&id);
            escaped.insert(id);
            overwrite_escaped.insert(id);
        }
    }
}

/// A field/index snapshot is only a cursor while its base object is still live. Last-use
/// destroy of `field` after `fname = field.name` (no retain) leaves `fname` dangling.
fn escape_cursors_outliving_base(
    func: &MirFunction,
    candidates: &mut HashSet<u32>,
    escaped: &mut HashSet<u32>,
    outlive_escaped: &mut HashSet<u32>,
    forwarding_copies: &[(u32, u32, usize, usize)],
) {
    let live_out = liveness::live_out(func);
    let mut copy_of: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for &(dest, src, _, _) in forwarding_copies {
        copy_of.insert(dest, src);
    }
    let mut snapshot_of: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
                if let Some(base) = snapshot_base(rvalue) {
                    snapshot_of.insert(dest.0, base);
                }
            }
        }
    }
    let peel = |mut x: u32| {
        let mut seen = HashSet::new();
        while seen.insert(x) {
            if let Some(&s) = copy_of.get(&x) {
                x = s;
            } else if let Some(&s) = snapshot_of.get(&x) {
                x = s;
            } else {
                break;
            }
        }
        x
    };
    let borrow_param = |l: u32| {
        func.params.iter().any(|p| p.0 == l) && !func.locals[l as usize].is_take
    };
    let sources: Vec<(u32, u32)> = func
        .blocks
        .iter()
        .flat_map(|block| &block.stmts)
        .filter_map(|stmt| match stmt {
            Statement::Assign(Place::Local(dest), rvalue) if candidates.contains(&dest.0) => {
                cursor_source_slot(rvalue).map(|slot| {
                    let base = match slot {
                        SourceSlot::Field(b, _)
                        | SourceSlot::IndexBase(b)
                        | SourceSlot::UnionBase(b) => b,
                    };
                    (dest.0, base)
                })
            }
            _ => None,
        })
        .collect();
    for (id, base) in sources {
        if id == base {
            continue;
        }
        // `this_2 = this;` then `m = this_2.obj_map`: `this_2` is dead at leftover of `m`,
        // but the borrow `this` still owns the object.
        if borrow_param(peel(base)) {
            continue;
        }
        let mut outlives = live_out
            .iter()
            .any(|out| out.contains(&id) && !out.contains(&base));
        if !outlives {
            for (bi, block) in func.blocks.iter().enumerate() {
                for (si, stmt) in block.stmts.iter().enumerate() {
                    let defines = matches!(
                        stmt,
                        Statement::Assign(Place::Local(d), _) if d.0 == id
                    );
                    if !defines {
                        continue;
                    }
                    if !base_consumed_after(block, base, si) {
                        continue;
                    }
                    let last_id = last_read_in_block(block, id);
                    if last_id.is_some_and(|li| li > si) || live_out[bi].contains(&id) {
                        outlives = true;
                        break;
                    }
                }
                if outlives {
                    break;
                }
            }
        }
        if outlives {
            candidates.remove(&id);
            escaped.insert(id);
            outlive_escaped.insert(id);
        }
    }
}

fn last_read_in_block(block: &crate::BasicBlock, local: u32) -> Option<usize> {
    let mut last = None;
    for (si, stmt) in block.stmts.iter().enumerate() {
        if stmt_reads_local(stmt, local) {
            last = Some(si);
        }
    }
    if terminator_reads_local(&block.terminator, local) {
        last = Some(block.stmts.len());
    }
    last
}

fn base_consumed_after(block: &crate::BasicBlock, base: u32, after_si: usize) -> bool {
    block.stmts.iter().enumerate().skip(after_si + 1).any(|(_, stmt)| {
        assigns_local_cursor(stmt, base) || stmt_sinks_local(stmt, base)
    })
}

fn assigns_local_cursor(stmt: &Statement, local: u32) -> bool {
    matches!(stmt, Statement::Assign(Place::Local(l), _) if l.0 == local)
}

/// Last-use of `local` at `stmt` transfers the +1 (sink call or container store), so a
/// snapshot of `local` cannot stay a cursor past this statement.
pub(crate) fn stmt_sinks_local(stmt: &Statement, local: u32) -> bool {
    if let Some((takes, args)) = call_take_args(stmt) {
        for (i, arg) in args.iter().enumerate() {
            if takes.get(i).copied().unwrap_or(false) && operand_mentions_local(arg, local) {
                return true;
            }
        }
    }
    matches!(
        stmt,
        Statement::Assign(
            Place::Field { .. } | Place::Index { .. } | Place::Global(_),
            Rvalue::Use(Operand::Copy(Place::Local(l))),
        ) if l.0 == local
    )
}

fn call_take_args(stmt: &Statement) -> Option<(Vec<bool>, &[Operand])> {
    match stmt {
        Statement::Call { callee, args } => Some((callee.take_params.clone(), args)),
        Statement::Assign(_, Rvalue::Call { callee, args, .. }) => {
            Some((callee.take_params.clone(), args))
        }
        Statement::Assign(
            _,
            Rvalue::New {
                ctor: Some(_),
                args,
                ..
            },
        ) => Some((vec![true; args.len()], args)),
        Statement::IndirectCall { args, .. } | Statement::InterfaceCall { args, .. } => {
            Some((vec![true; args.len()], args))
        }
        Statement::Assign(_, Rvalue::IndirectCall { args, .. })
        | Statement::Assign(_, Rvalue::InterfaceCall { args, .. }) => {
            Some((vec![true; args.len()], args))
        }
        _ => None,
    }
}

fn terminator_reads_local(term: &Terminator, local: u32) -> bool {
    match term {
        Terminator::If { cond, .. } => operand_mentions_local(cond, local),
        Terminator::Switch { value, .. } => operand_mentions_local(value, local),
        Terminator::Return(Some(o)) | Terminator::AsyncComplete(Some(o)) => {
            operand_mentions_local(o, local)
        }
        Terminator::TailCall { args, .. } => args.iter().any(|a| operand_mentions_local(a, local)),
        Terminator::Await { future, .. } => operand_mentions_local(future, local),
        _ => false,
    }
}

fn operand_mentions_local(op: &Operand, local: u32) -> bool {
    match op {
        Operand::Copy(Place::Local(l)) | Operand::Copy(Place::Deref { ptr: l, .. }) => {
            l.0 == local
        }
        Operand::Copy(Place::Field { base, .. }) => base.0 == local,
        Operand::Copy(Place::Index { base, index, .. }) => {
            base.0 == local || operand_mentions_local(index, local)
        }
        _ => false,
    }
}

fn mark_stmt_escapes(stmt: &Statement, escaped: &mut HashSet<u32>) {
    match stmt {
        Statement::Assign(Place::Field { .. }, rvalue)
        | Statement::Assign(Place::Index { .. }, rvalue)
        | Statement::Assign(Place::Global(_), rvalue) => {
            escape_rvalue_payload(rvalue, escaped);
        }
        Statement::Assign(Place::Local(_), rvalue) => {
            escape_constructor_payloads(rvalue, escaped);
        }
        Statement::Call { callee, args } => escape_take_args(callee, args, escaped),
        Statement::IndirectCall { args, .. } | Statement::InterfaceCall { args, .. } => {
            for a in args {
                escape_operand(a, escaped);
            }
        }
        Statement::JsCall { args, .. } => {
            for (a, _) in args {
                escape_operand(a, escaped);
            }
        }
        Statement::ForceFree(op) => escape_operand(op, escaped),
        Statement::ArrayElemsCopy { dst, src, .. } => {
            escape_operand(dst, escaped);
            escape_operand(src, escaped);
        }
        Statement::ArrayElemsFill { dst, .. } => escape_operand(dst, escaped),
        _ => {}
    }
}

fn mark_term_escapes(term: &Terminator, escaped: &mut HashSet<u32>) {
    match term {
        Terminator::Return(Some(op)) | Terminator::AsyncComplete(Some(op)) => {
            escape_operand(op, escaped);
        }
        Terminator::TailCall { callee, args } => escape_take_args(callee, args, escaped),
        Terminator::Await { future, .. } => escape_operand(future, escaped),
        _ => {}
    }
}

fn escape_take_args(callee: &Callee, args: &[Operand], escaped: &mut HashSet<u32>) {
    for (i, arg) in args.iter().enumerate() {
        if callee.take_params.get(i).copied().unwrap_or(false) {
            escape_operand(arg, escaped);
        }
    }
}

fn escape_constructor_payloads(rvalue: &Rvalue, escaped: &mut HashSet<u32>) {
    match rvalue {
        Rvalue::Call { callee, args, .. } => escape_take_args(callee, args, escaped),
        Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::ArrayLit { elems: args, .. }
        | Rvalue::Tuple { elems: args, .. } => {
            for a in args {
                escape_operand(a, escaped);
            }
        }
        Rvalue::IndirectCall { args, .. } | Rvalue::InterfaceCall { args, .. } => {
            for a in args {
                escape_operand(a, escaped);
            }
        }
        Rvalue::JsCall { args, .. } => {
            for (a, _) in args {
                escape_operand(a, escaped);
            }
        }
        _ => {}
    }
}

fn escape_rvalue_payload(rvalue: &Rvalue, escaped: &mut HashSet<u32>) {
    match rvalue {
        Rvalue::Use(op) | Rvalue::Cast(op, _, _) => escape_operand(op, escaped),
        other => escape_constructor_payloads(other, escaped),
    }
}

fn escape_operand(op: &Operand, escaped: &mut HashSet<u32>) {
    if let Operand::Copy(Place::Local(l)) = op {
        escaped.insert(l.0);
    }
}
