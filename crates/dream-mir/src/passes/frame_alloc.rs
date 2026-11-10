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
//! A `t = s.substring(a, b)` slice is built the same way. Instead of the slice retaining `s`, a
//! copy of `s` is retained where the slice is built and released at each of its deaths, which is
//! what the heap slice's own destroy would have done. When the slice dies in the block that builds
//! it and nothing in between can drop a count, the parent's existing holder suffices and the copy
//! is skipped.
//!
//! Refused: `del` (it would have to run at the death), weak / unowned fields (registry entries
//! point into the block), `@shared`, value fields with managed contents, instances over
//! [`MAX_OBJECT_BYTES`] (arrays over [`MAX_ARRAY_BYTES`]), functions on a call cycle (frame growth under recursion) or opening a
//! unique region (its rewind frees what the deaths would release again), and async bodies.

use crate::analysis::effects::strong_children;
use crate::analysis::escape::{Escape, LocalEscape, ParamSummaries};
use crate::analysis::object_life::{Lifetime, lifetime};
use crate::{Const, Local, LocalDecl, Mir, MirFunction, Operand, Place, Rvalue, Statement};
use dream_abi::intrinsics::IntrinsicOp;
use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet};

/// `--emit-mir` stage name.
pub(crate) const STAGE: &str = "frame-alloc";

const MAX_OBJECT_BYTES: u32 = 256;
const MAX_ARRAY_BYTES: u32 = 512;
const MAX_FRAME_BYTES: u32 = 1024;
/// A string slice after its header: length and kind words, parent and units pointers, and the
/// native cached hash. Sized for 64-bit pointers on every target.
const SLICE_PAYLOAD: u32 = 8 + 2 * 8 + 4;

pub(crate) fn run(mir: &mut Mir, interner: &TypeInterner) -> bool {
    let sums = ParamSummaries::compute(mir, interner);
    let substrings: BTreeSet<_> = mir
        .intrinsics
        .iter()
        .filter(|(_, key)| IntrinsicOp::from_key(key) == Some(IntrinsicOp::StringSubstring))
        .map(|(def, _)| *def)
        .collect();
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
            let mut parent = None;
            let (fields, payload, limit) = match rv {
                Rvalue::Call { callee, args }
                    if substrings.contains(&callee.def)
                        && args.len() == 3
                        && !callee.take_params.iter().any(|t| *t) =>
                {
                    parent = Some(args[0].clone());
                    (Vec::new(), SLICE_PAYLOAD, MAX_OBJECT_BYTES)
                }
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
            let free_deaths = fields.is_empty() && parent.is_none();
            let Some(life) = lifetime(f, &esc.class(*o), *o, free_deaths, parent.is_some()) else {
                continue;
            };
            let parent = parent.filter(|p| !parent_outlives(f, *o, p, &life));
            budget -= size;
            tracing::debug!(pass = STAGE, function = %f.name, local = o.0,
                decision = "eliminated", cost = "heap-allocation", frame_bytes = size);
            plans.push(Plan {
                object: *o,
                fields,
                parent,
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
    /// The string a frame slice views, held by the function instead of by the slice.
    parent: Option<Operand>,
    life: Lifetime,
    /// Field bytes of the constructed class, after the heap header.
    payload: u32,
}

/// True when nothing can drop the slice's parent while the slice lives: the slice is built once,
/// dies in the same block, and the statements in between neither release (except the slice's own
/// RC ops), call, store, nor rebind the parent. The parent's existing holder then keeps it alive.
fn parent_outlives(f: &MirFunction, o: Local, parent: &Operand, life: &Lifetime) -> bool {
    let s = match parent {
        Operand::Const(_) => None,
        Operand::Copy(Place::Local(s)) => Some(*s),
        _ => return false,
    };
    let mut sites = f.blocks.iter().enumerate().flat_map(|(bi, b)| {
        b.stmts.iter().enumerate().filter_map(move |(si, st)| {
            matches!(st, Statement::Assign(Place::Local(d), Rvalue::Call { .. }) if *d == o)
                .then_some((bi, si))
        })
    });
    let (Some((bi, start)), None) = (sites.next(), sites.next()) else {
        return false;
    };
    let Some(end) = life
        .deaths
        .iter()
        .map(|&(db, ds, _)| (db == bi && ds > start).then_some(ds))
        .try_fold(start, |end, d| d.map(|d| end.max(d)))
    else {
        return false;
    };
    let own = |si: usize| {
        life.rc_ops.contains(&(bi, si)) || life.deaths.iter().any(|d| (d.0, d.1) == (bi, si))
    };
    f.blocks[bi].stmts[start + 1..end]
        .iter()
        .enumerate()
        .all(|(k, st)| match st {
            Statement::Retain(_)
            | Statement::Panic(_)
            | Statement::Nop
            | Statement::DebugLine(_)
            | Statement::SourceLine(_) => true,
            Statement::Release(_) => own(start + 1 + k),
            Statement::Assign(Place::Local(d), rv) => Some(*d) != s && count_neutral(rv),
            _ => false,
        })
}

/// Rvalues that run no user code and change no reference count.
fn count_neutral(rv: &Rvalue) -> bool {
    matches!(
        rv,
        Rvalue::Zero(_)
            | Rvalue::Use(_)
            | Rvalue::Move { .. }
            | Rvalue::Select { .. }
            | Rvalue::Binary(..)
            | Rvalue::Unary(..)
            | Rvalue::CheckedBinary(..)
            | Rvalue::CheckedNeg(_)
            | Rvalue::StrLen(_)
            | Rvalue::StrByteSize(_)
            | Rvalue::CharAt(..)
            | Rvalue::ByteAt(..)
            | Rvalue::LoadU8(..)
            | Rvalue::LoadU16(..)
            | Rvalue::ArrayLen(_)
            | Rvalue::Cast(..)
            | Rvalue::IsType(..)
    )
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

type Death<'p> = (Local, TypeId, &'p RefFields, Option<Local>);

fn apply(f: &mut MirFunction, plans: &[Plan]) {
    let mut drop: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut deaths: BTreeMap<(usize, usize), Death> = BTreeMap::new();
    let mut holds: BTreeMap<Local, (Local, Operand)> = BTreeMap::new();
    for p in plans {
        drop.extend(p.life.rc_ops.iter().copied());
        let class = f.locals[p.object.0 as usize].ty;
        let held = p.parent.as_ref().map(|parent| {
            let t = push_temp(f, class);
            holds.insert(p.object, (t, parent.clone()));
            t
        });
        for &(bi, si, m) in &p.life.deaths {
            deaths.insert((bi, si), (m, class, &p.fields, held));
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
        .filter(|&&(m, class, _, held)| held.is_none() && f.locals[m.0 as usize].ty != class)
        .map(|&(_, class, _, _)| class)
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
            if let Statement::Assign(Place::Local(o), Rvalue::Call { .. }) = &s
                && let Some((held, parent)) = holds.get(o)
            {
                block.stmts.push(Statement::Assign(
                    Place::Local(*held),
                    Rvalue::Use(parent.clone()),
                ));
                block
                    .stmts
                    .push(Statement::Retain(Operand::Copy(Place::Local(*held))));
            }
            let Some(&(m, class, fields, held)) = deaths.get(&(bi, si)) else {
                block.stmts.push(s);
                continue;
            };
            if let Some(held) = held {
                block
                    .stmts
                    .push(Statement::Release(Operand::Copy(Place::Local(held))));
                continue;
            }
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
