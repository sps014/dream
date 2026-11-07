//! Frame allocation of class instances that never outlive their function.
//!
//! An `o = New { ty }` whose alias class is at most [`Escape::Arg`] and whose count is static
//! ([`lifetime`]) is built in a buffer of the function's C frame instead of the heap:
//! `dream_frame_object` gives it a live header with an immortal count, so retains and releases in
//! callees are no-ops and nothing frees the block. The class's own RC statements go, and each
//! death becomes the destroy glue minus the free: a release of every strong field.
//!
//! An `a = ArrayNew` with a constant length and elements that hold no references gets the same
//! treatment; its deaths release nothing.
//!
//! Refused: `del` (it would have to run at the death), weak / unowned fields (registry entries
//! point into the block), `@shared`, value fields with managed contents, instances over
//! [`MAX_OBJECT_BYTES`] (arrays over [`MAX_ARRAY_BYTES`]), functions on a call cycle (frame growth under recursion) or opening a
//! unique region (its rewind frees what the deaths would release again), and async bodies.

use crate::analysis::effects::strong_children;
use crate::analysis::escape::{Escape, LocalEscape, ParamSummaries};
use crate::analysis::object_life::{Lifetime, lifetime};
use crate::{Const, Local, LocalDecl, Mir, MirFunction, Operand, Place, Rvalue, Statement};
use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet};

/// `--emit-mir` stage name.
pub(crate) const STAGE: &str = "frame-alloc";

const MAX_OBJECT_BYTES: u32 = 256;
const MAX_ARRAY_BYTES: u32 = 512;
const MAX_FRAME_BYTES: u32 = 1024;

pub(crate) fn run(mir: &mut Mir, interner: &TypeInterner) -> bool {
    let sums = ParamSummaries::compute(mir, interner);
    let layouts = &mir.layouts;
    let mut marked = Vec::new();
    let mut hoisted = false;
    for f in &mut mir.functions {
        if f.is_async
            || sums.is_recursive(f.def, &f.instance)
            || f.blocks
                .iter()
                .flat_map(|b| &b.stmts)
                .any(|s| matches!(s, Statement::RegionEnter | Statement::RegionLeave))
        {
            continue;
        }
        let esc = LocalEscape::analyze(f, interner, &sums);
        hoisted |= hoist_loop_releases(f, &esc, interner, layouts);
        let mut plans: Vec<Plan> = Vec::new();
        let mut budget = MAX_FRAME_BYTES;
        for s in f.blocks.iter().flat_map(|b| &b.stmts) {
            let Statement::Assign(Place::Local(o), rv) = s else {
                continue;
            };
            let (fields, payload, limit) = match rv {
                Rvalue::New { ty, .. } => {
                    let Some(fields) = releasable_fields(*ty, interner, layouts) else {
                        continue;
                    };
                    (
                        fields,
                        layouts.get(*ty).map_or(u32::MAX, |l| l.size),
                        MAX_OBJECT_BYTES,
                    )
                }
                Rvalue::ArrayNew {
                    elem_ty,
                    len,
                    closure_env: false,
                } => {
                    let Some(payload) = array_payload(f, *elem_ty, len, interner, layouts) else {
                        continue;
                    };
                    (Vec::new(), payload, MAX_ARRAY_BYTES)
                }
                _ => continue,
            };
            if esc.of(*o) > Escape::Arg {
                tracing::debug!(pass = STAGE, function = %f.name, local = o.0,
                    decision = "rejected", reason = "possible-escape-or-publication");
                continue;
            }
            // Each backend buffer includes the immortal ARC header and is eight-aligned.
            let size = payload
                .checked_add(crate::abi::HEAP_HEADER_SIZE)
                .and_then(|n| n.checked_add(7))
                .map(|n| n & !7)
                .unwrap_or(u32::MAX);
            if size > limit || size > budget {
                tracing::debug!(pass = STAGE, function = %f.name, local = o.0,
                    decision = "rejected", reason = "frame-budget-limit", size, budget);
                continue;
            }
            let Some(life) = lifetime(f, &esc.class(*o), *o, fields.is_empty()) else {
                continue;
            };
            budget -= size;
            tracing::debug!(pass = STAGE, function = %f.name, local = o.0,
                decision = "eliminated", cost = "heap-allocation", frame_bytes = size);
            plans.push(Plan {
                object: *o,
                fields,
                life,
                payload,
            });
        }
        if plans.is_empty() {
            continue;
        }
        apply(f, &plans);
        marked.extend(
            plans
                .iter()
                .map(|p| ((f.def, f.instance.clone(), p.object), p.payload)),
        );
    }
    let changed = hoisted || !marked.is_empty();
    for (key, payload) in marked {
        let slot = mir.frame_objects.entry(key).or_insert(0);
        *slot = (*slot).max(payload);
    }
    changed
}

/// A loop that reallocates a frameable array into the same holder allocates the next instance
/// before releasing the previous one, so two would be live at once. The statements between them
/// only copy or clear holders, and the elements hold no references, so the release moves above
/// the allocation without changing what any statement observes.
fn hoist_loop_releases(
    f: &mut MirFunction,
    esc: &LocalEscape,
    interner: &TypeInterner,
    layouts: &LayoutTable,
) -> bool {
    let mut moves = Vec::new();
    for (bi, block) in f.blocks.iter().enumerate() {
        for (si, s) in block.stmts.iter().enumerate() {
            let Statement::Assign(
                Place::Local(o),
                Rvalue::ArrayNew {
                    elem_ty,
                    len,
                    closure_env: false,
                },
            ) = s
            else {
                continue;
            };
            if esc.of(*o) > Escape::Arg
                || array_payload(f, *elem_ty, len, interner, layouts).is_none()
            {
                continue;
            }
            let class = esc.class(*o);
            let member = |op: &Operand| match op {
                Operand::Copy(Place::Local(l)) if class.contains(l) => Some(*l),
                _ => None,
            };
            let mut touched = vec![*o];
            for (j, next) in block.stmts.iter().enumerate().skip(si + 1) {
                match next {
                    Statement::Release(op) => {
                        if let Some(m) = member(op)
                            && !touched.contains(&m)
                        {
                            moves.push((bi, si, j));
                        }
                        break;
                    }
                    Statement::Assign(Place::Local(d), rv) if class.contains(d) => {
                        let src = match rv {
                            Rvalue::Use(Operand::Const(Const::Null)) => None,
                            Rvalue::Move { src, cast: None } => Some(*src),
                            Rvalue::Use(op) => match member(op) {
                                Some(m) => Some(m),
                                None => break,
                            },
                            _ => break,
                        };
                        touched.push(*d);
                        touched.extend(src);
                    }
                    Statement::SourceLine(_) | Statement::DebugLine(_) | Statement::Nop => {}
                    _ => break,
                }
            }
        }
    }
    for &(bi, si, j) in moves.iter().rev() {
        let release = f.blocks[bi].stmts.remove(j);
        f.blocks[bi].stmts.insert(si, release);
    }
    !moves.is_empty()
}

/// `(field index, type)` of every strong reference field.
type RefFields = [(usize, TypeId)];

struct Plan {
    object: Local,
    fields: Vec<(usize, TypeId)>,
    life: Lifetime,
    /// Field bytes of the constructed class, after the heap header.
    payload: u32,
}

/// The strong reference fields a death must release, or `None` if the type is not frameable.
fn releasable_fields(
    ty: TypeId,
    interner: &TypeInterner,
    layouts: &LayoutTable,
) -> Option<Vec<(usize, TypeId)>> {
    if !matches!(interner.kind(ty), TyKind::Struct(..))
        || interner.is_value_type(ty)
        || interner.is_shared_type(ty)
    {
        return None;
    }
    let layout = layouts.get(ty)?;
    // LocalEscape rejects every store, return, weak observation or opaque call that
    // could retain this allocation. A cycle through it requires such an incoming edge;
    // cycle-capable types can therefore use the same local lifetime/field-cleanup proof.
    if layout.has_destructor() {
        return None;
    }
    let mut out = Vec::new();
    for (i, fl) in layout.fields.iter().enumerate() {
        if fl.is_weak || fl.is_unowned {
            return None;
        }
        if interner.is_reference(fl.ty) {
            out.push((i, fl.ty));
        } else if !unmanaged(fl.ty, interner, layouts, 0) {
            return None;
        }
    }
    Some(out)
}

/// Bytes after the heap header of a constant-length array whose elements hold no references:
/// the length word, then the elements.
fn array_payload(
    f: &MirFunction,
    elem: TypeId,
    len: &Operand,
    interner: &TypeInterner,
    layouts: &LayoutTable,
) -> Option<u32> {
    if !unmanaged(elem, interner, layouts, 0) {
        return None;
    }
    let n = u32::try_from(const_int(f, len)?).ok()?;
    let (size, _) = layouts.size_align(interner, elem);
    size.max(1)
        .checked_mul(n)?
        .checked_add(crate::abi::LEN_PREFIX_SIZE)
}

/// The value of an integer literal operand, or of a non-parameter local whose only definition is one.
fn const_int(f: &MirFunction, op: &Operand) -> Option<i64> {
    match op {
        Operand::Const(Const::Int(n)) => Some(*n),
        Operand::Copy(Place::Local(l)) if !f.params.contains(l) => {
            let mut value = None;
            for s in f.blocks.iter().flat_map(|b| &b.stmts) {
                if let Statement::Assign(Place::Local(d), rv) = s
                    && d == l
                {
                    let Rvalue::Use(Operand::Const(Const::Int(n))) = rv else {
                        return None;
                    };
                    if value.replace(*n).is_some() {
                        return None;
                    }
                }
            }
            value
        }
        _ => None,
    }
}

fn unmanaged(ty: TypeId, interner: &TypeInterner, layouts: &LayoutTable, depth: u32) -> bool {
    if interner.is_reference(ty) || depth > 8 {
        return false;
    }
    match interner.kind(ty) {
        TyKind::Prim(_) | TyKind::Enum(_) | TyKind::Void => true,
        _ => strong_children(ty, interner, layouts).is_some_and(|cs| {
            cs.iter()
                .all(|&c| unmanaged(c, interner, layouts, depth + 1))
        }),
    }
}

fn apply(f: &mut MirFunction, plans: &[Plan]) {
    let mut drop: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut deaths: BTreeMap<(usize, usize), (Local, TypeId, &RefFields)> = BTreeMap::new();
    for p in plans {
        drop.extend(p.life.rc_ops.iter().copied());
        let class = f.locals[p.object.0 as usize].ty;
        for &(bi, si, m) in &p.life.deaths {
            deaths.insert((bi, si), (m, class, &p.fields));
        }
    }
    let mut temps: BTreeMap<TypeId, Local> = BTreeMap::new();
    for &(_, ty) in plans.iter().flat_map(|p| &p.fields) {
        temps.entry(ty).or_insert_with(|| push_temp(f, ty));
    }
    // A death may be reached through an alias typed `Option<Class>`; field offsets come from the
    // class layout, so such deaths project through a temporary of the class type.
    let mut views: BTreeMap<TypeId, Local> = BTreeMap::new();
    let aliased: Vec<TypeId> = deaths
        .values()
        .filter(|&&(m, class, _)| f.locals[m.0 as usize].ty != class)
        .map(|&(_, class, _)| class)
        .collect();
    for class in aliased {
        views.entry(class).or_insert_with(|| push_temp(f, class));
    }
    let local_tys: Vec<TypeId> = f.locals.iter().map(|l| l.ty).collect();
    for (bi, block) in f.blocks.iter_mut().enumerate() {
        let old = std::mem::take(&mut block.stmts);
        for (si, s) in old.into_iter().enumerate() {
            if drop.contains(&(bi, si)) {
                continue;
            }
            let Some(&(m, class, fields)) = deaths.get(&(bi, si)) else {
                block.stmts.push(s);
                continue;
            };
            let base = if local_tys[m.0 as usize] == class {
                m
            } else {
                let view = views[&class];
                block.stmts.push(Statement::Assign(
                    Place::Local(view),
                    Rvalue::Use(Operand::Copy(Place::Local(m))),
                ));
                view
            };
            for &(field, ty) in fields {
                let t = temps[&ty];
                block.stmts.push(Statement::Assign(
                    Place::Local(t),
                    Rvalue::Use(Operand::Copy(Place::Field { base, field })),
                ));
                block
                    .stmts
                    .push(Statement::Release(Operand::Copy(Place::Local(t))));
            }
        }
    }
}

fn push_temp(f: &mut MirFunction, ty: TypeId) -> Local {
    f.locals.push(LocalDecl {
        ty,
        name: None,
        is_ref: false,
        is_take: false,
        is_cursor: false,
        manual_drop: false,
        borrows_refs: false,
    });
    Local(f.locals.len() as u32 - 1)
}
