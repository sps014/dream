//! Frame allocation of class instances that never outlive their function.
//!
//! An `o = New { ty }` whose alias class is at most [`Escape::Arg`] and whose count is static
//! ([`lifetime`]) is built in a buffer of the function's C frame instead of the heap:
//! `dream_frame_object` gives it a live header with an immortal count, so retains and releases in
//! callees are no-ops and nothing frees the block. The class's own RC statements go, and each
//! death becomes the destroy glue minus the free: a release of every strong field.
//!
//! Refused: `del` (it would have to run at the death), weak / unowned fields (registry entries
//! point into the block), `@shared`, value fields with managed contents, instances over
//! [`MAX_OBJECT_BYTES`], functions on a call cycle (frame growth under recursion) or opening a
//! unique region (its rewind frees what the deaths would release again), and async bodies.

use crate::analysis::escape::{Escape, LocalEscape, ParamSummaries};
use crate::analysis::object_life::{lifetime, Lifetime};
use crate::passes::rc::modref::strong_children;
use crate::{Local, LocalDecl, Mir, MirFunction, Operand, Place, Rvalue, Statement};
use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet};

/// `--emit-mir` stage name.
pub(crate) const STAGE: &str = "frame-alloc";

const MAX_OBJECT_BYTES: u32 = 256;
const MAX_FRAME_BYTES: u32 = 1024;

pub(crate) fn run(mir: &mut Mir, interner: &TypeInterner) -> bool {
    let sums = ParamSummaries::compute(mir, interner);
    let dels: BTreeSet<String> = mir
        .functions
        .iter()
        .filter_map(|f| f.name.strip_suffix("_del").map(str::to_string))
        .collect();
    let layouts = &mir.layouts;
    let mut marked = Vec::new();
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
        let mut plans: Vec<Plan> = Vec::new();
        let mut budget = MAX_FRAME_BYTES;
        for s in f.blocks.iter().flat_map(|b| &b.stmts) {
            let Statement::Assign(Place::Local(o), Rvalue::New { ty, .. }) = s else {
                continue;
            };
            if esc.of(*o) > Escape::Arg {
                continue;
            }
            let Some(fields) = releasable_fields(*ty, interner, layouts, &dels) else {
                continue;
            };
            let size = layouts.get(*ty).map_or(u32::MAX, |l| l.size);
            if size > MAX_OBJECT_BYTES || size > budget {
                continue;
            }
            let Some(life) = lifetime(f, &esc.class(*o), *o) else {
                continue;
            };
            budget -= size;
            plans.push(Plan {
                object: *o,
                fields,
                life,
            });
        }
        if plans.is_empty() {
            continue;
        }
        apply(f, &plans);
        marked.extend(plans.iter().map(|p| (f.def, f.instance.clone(), p.object)));
    }
    let changed = !marked.is_empty();
    mir.frame_objects.extend(marked);
    changed
}

/// `(field index, type)` of every strong reference field.
type RefFields = [(usize, TypeId)];

struct Plan {
    object: Local,
    fields: Vec<(usize, TypeId)>,
    life: Lifetime,
}

/// The strong reference fields a death must release, or `None` if the type is not frameable.
fn releasable_fields(
    ty: TypeId,
    interner: &TypeInterner,
    layouts: &LayoutTable,
    dels: &BTreeSet<String>,
) -> Option<Vec<(usize, TypeId)>> {
    if !matches!(interner.kind(ty), TyKind::Struct(..))
        || interner.is_value_type(ty)
        || interner.is_shared_type(ty)
    {
        return None;
    }
    let layout = layouts.get(ty)?;
    if dels.contains(&layout.name) {
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

fn unmanaged(ty: TypeId, interner: &TypeInterner, layouts: &LayoutTable, depth: u32) -> bool {
    if interner.is_reference(ty) || depth > 8 {
        return false;
    }
    match interner.kind(ty) {
        TyKind::Prim(_) | TyKind::Enum(_) | TyKind::Void => true,
        _ => strong_children(ty, interner, layouts)
            .is_some_and(|cs| cs.iter().all(|&c| unmanaged(c, interner, layouts, depth + 1))),
    }
}

fn apply(f: &mut MirFunction, plans: &[Plan]) {
    let mut drop: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut deaths: BTreeMap<(usize, usize), (Local, &RefFields)> = BTreeMap::new();
    for p in plans {
        drop.extend(p.life.rc_ops.iter().copied());
        for &(bi, si, m) in &p.life.deaths {
            deaths.insert((bi, si), (m, &p.fields));
        }
    }
    let mut temps: BTreeMap<TypeId, Local> = BTreeMap::new();
    for &(_, ty) in plans.iter().flat_map(|p| &p.fields) {
        temps.entry(ty).or_insert_with(|| {
            f.locals.push(LocalDecl {
                ty,
                name: None,
                is_ref: false,
                is_take: false,
                is_cursor: false,
                manual_drop: false,
            });
            Local(f.locals.len() as u32 - 1)
        });
    }
    for (bi, block) in f.blocks.iter_mut().enumerate() {
        let old = std::mem::take(&mut block.stmts);
        for (si, s) in old.into_iter().enumerate() {
            if drop.contains(&(bi, si)) {
                continue;
            }
            let Some(&(m, fields)) = deaths.get(&(bi, si)) else {
                block.stmts.push(s);
                continue;
            };
            for &(field, ty) in fields {
                let t = temps[&ty];
                block.stmts.push(Statement::Assign(
                    Place::Local(t),
                    Rvalue::Use(Operand::Copy(Place::Field { base: m, field })),
                ));
                block
                    .stmts
                    .push(Statement::Release(Operand::Copy(Place::Local(t))));
            }
        }
    }
}
