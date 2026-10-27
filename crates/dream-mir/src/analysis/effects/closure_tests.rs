use super::local::{Edge, LocalSummary};
use super::{Known, ModRef, ModRefTable};
use crate::Mir;
use crate::build::FunctionBuilder;
use dream_hir::InterfaceImpl;
use dream_types::{DefId, TypeCtx};
use indexmap::IndexMap;

// Independent full rescans serve as an oracle for the dependency graph, including cycles.
fn rescan(table: &mut ModRefTable, mir: &Mir, locals: &[LocalSummary]) {
    loop {
        let mut grew = false;
        let mut interfaces: IndexMap<(usize, usize), ModRef> = IndexMap::new();
        for implementation in &mir.interfaces.impls {
            for (id, slots) in &implementation.entries {
                for (slot, def) in slots.iter().enumerate() {
                    let mut summary = ModRef::default();
                    let mut found = false;
                    for f in &mir.functions {
                        if Some(f.def) == *def {
                            found = true;
                            summary.join(&table.call_def(f.def, &f.instance));
                        }
                    }
                    if !found {
                        summary = ModRef::Top;
                    }
                    interfaces.entry((*id, slot)).or_default().join(&summary);
                }
            }
        }
        for local in locals {
            for edge in &local.edges {
                if let Edge::Iface(id, slot) = edge {
                    interfaces.entry((*id, *slot)).or_insert(ModRef::Top);
                }
            }
        }
        grew |= interfaces != table.ifaces;
        table.ifaces = interfaces;
        for (f, local) in mir.functions.iter().zip(locals) {
            let mut normal = local.own.clone();
            let mut fresh = local.own_fresh.clone();
            for edge in &local.edges {
                let effect = match edge {
                    Edge::Call(def, args, parameters) => {
                        table.call_def(*def, args).mapped(parameters)
                    }
                    Edge::Ctor(def) => table.ctor(*def).mapped(&[]),
                    Edge::Iface(id, slot) => table.iface(*id, *slot).mapped(&[]),
                };
                normal.join(&effect);
                fresh.join(&effect);
            }
            grew |= table
                .fns
                .get_mut(&(f.def, f.instance.clone()))
                .unwrap()
                .join(&normal);
            grew |= table.ctors.get_mut(&f.def).unwrap().join(&fresh);
        }
        if !grew {
            return;
        }
    }
}

fn initial(mir: &Mir, locals: &[LocalSummary]) -> ModRefTable {
    let mut table = ModRefTable::default();
    for (f, local) in mir.functions.iter().zip(locals) {
        table
            .fns
            .insert((f.def, f.instance.clone()), local.own.clone());
        table.ctors.entry(f.def).or_default().join(&local.own_fresh);
    }
    table
}

#[test]
fn dependency_closure_matches_rescans_on_recursive_and_erased_graphs() {
    let ctx = TypeCtx::new();
    let mut seed = 9_u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as usize
    };
    for _ in 0..100 {
        let mut mir = Mir::default();
        for index in 0..24 {
            let mut b = FunctionBuilder::new("effect", ctx.interner.void());
            b.set_def(
                DefId::root((index / 2) as u32),
                vec![if index % 2 == 0 {
                    ctx.interner.int()
                } else {
                    ctx.interner.string()
                }],
            );
            mir.functions.push(b.finish());
        }
        for _ in 0..4 {
            let slots = (0..3)
                .map(|_| match next() % 16 {
                    15 => None,
                    def => Some(DefId::root(def as u32)),
                })
                .collect();
            mir.interfaces.impls.push(InterfaceImpl {
                class_ty: ctx.interner.object(),
                entries: vec![(0, slots)],
            });
        }
        let mut locals = vec![];
        for _ in &mir.functions {
            let mut known = Known {
                observes_rc: next() % 11 == 0,
                may_destroy: next() % 13 == 0,
                ..Known::default()
            };
            if next() % 3 == 0 {
                known
                    .fields
                    .insert((ctx.interner.object(), (next() % 6) as u32));
            }
            let mut edges = vec![];
            for _ in 0..next() % 4 {
                let target = &mir.functions[next() % mir.functions.len()];
                edges.push(match next() % 4 {
                    0 => Edge::Iface(next() % 2, next() % 4),
                    1 => Edge::Ctor(target.def),
                    2 => Edge::Call(DefId::root(100), vec![], vec![]),
                    _ => Edge::Call(target.def, target.instance.clone(), vec![]),
                });
            }
            locals.push(LocalSummary {
                observes_rc: false,
                may_destroy: false,
                own: ModRef::Known(known),
                own_fresh: ModRef::default(),
                edges,
            });
        }
        let mut expected = initial(&mir, &locals);
        rescan(&mut expected, &mir, &locals);
        let mut actual = initial(&mir, &locals);
        actual.close_effects(&mir, &locals);
        assert_eq!(actual.fns, expected.fns);
        assert_eq!(actual.ctors, expected.ctors);
        assert_eq!(actual.ifaces, expected.ifaces);
    }
}

#[test]
fn reversed_call_chains_require_linear_summary_evaluations() {
    let ctx = TypeCtx::new();
    for size in [32, 128, 512] {
        let mut mir = Mir::default();
        let mut locals = vec![];
        for index in 0..size {
            let mut b = FunctionBuilder::new("chain", ctx.interner.void());
            b.set_def(DefId::root(index as u32), vec![]);
            mir.functions.push(b.finish());
            let mut known = Known::default();
            let edges = if index + 1 == size {
                known.fields.insert((ctx.interner.object(), 0));
                vec![]
            } else {
                vec![Edge::Call(DefId::root((index + 1) as u32), vec![], vec![])]
            };
            locals.push(LocalSummary {
                observes_rc: false,
                may_destroy: false,
                own: ModRef::Known(known),
                own_fresh: ModRef::default(),
                edges,
            });
        }
        let mut expected = initial(&mir, &locals);
        rescan(&mut expected, &mir, &locals);
        let mut actual = initial(&mir, &locals);
        let evaluations = actual.close_effects(&mir, &locals);
        assert!(
            evaluations <= 6 * size,
            "size={size}, evaluations={evaluations}"
        );
        assert_eq!(actual.fns, expected.fns);
        assert_eq!(actual.ctors, expected.ctors);
    }
}
