//! Borrowed getter results remain aliases only while their receiver's strong path is intact.

use super::liveness;
use crate::analysis::effects::{Effect, ModRef, ModRefTable, stmt_effects};
use crate::{Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
enum Slot {
    Field(TypeId, u32),
    Element(TypeId),
    Union(TypeId),
}

#[derive(Default)]
struct Reads {
    fields: BTreeSet<(TypeId, u32)>,
    elements: BTreeSet<TypeId>,
    unions: BTreeSet<TypeId>,
    opaque: bool,
}

impl Reads {
    fn of(receiver: TypeId, result: TypeId, i: &TypeInterner, layouts: &LayoutTable) -> Self {
        let result = if i.is_niche_union(result) {
            layouts
                .union(result)
                .and_then(|u| u.variants.iter().flat_map(|v| &v.fields).next())
                .map_or(result, |f| f.ty)
        } else {
            result
        };
        let mut graph = BTreeMap::new();
        let mut opaque = BTreeSet::new();
        let mut pending = vec![receiver];
        while let Some(ty) = pending.pop() {
            if graph.contains_key(&ty) {
                continue;
            }
            let edges: Vec<_> = if ty == result {
                vec![]
            } else {
                match i.kind(ty) {
                    TyKind::Array(element) => vec![(*element, Slot::Element(*element))],
                    TyKind::Struct(..) => match layouts.get(ty) {
                        Some(layout) => layout
                            .fields
                            .iter()
                            .enumerate()
                            .filter(|(_, f)| !f.is_weak && !f.is_unowned)
                            .map(|(index, f)| (f.ty, Slot::Field(ty, index as u32)))
                            .collect(),
                        None => {
                            opaque.insert(ty);
                            vec![]
                        }
                    },
                    TyKind::Union(..) => match layouts.union(ty) {
                        Some(layout) => layout
                            .variants
                            .iter()
                            .flat_map(|v| &v.fields)
                            .filter(|f| !f.is_weak && !f.is_unowned)
                            .map(|f| (f.ty, Slot::Union(ty)))
                            .collect(),
                        None => {
                            opaque.insert(ty);
                            vec![]
                        }
                    },
                    TyKind::Tuple(elements) => elements
                        .iter()
                        .enumerate()
                        .map(|(index, &element)| (element, Slot::Field(ty, index as u32)))
                        .collect(),
                    TyKind::Object | TyKind::Interface(..) | TyKind::Func(..) | TyKind::Js => {
                        opaque.insert(ty);
                        vec![]
                    }
                    _ => vec![],
                }
            };
            pending.extend(edges.iter().map(|(child, _)| *child));
            graph.insert(ty, edges);
        }
        let mut reaches = opaque.clone();
        reaches.insert(result);
        loop {
            let before = reaches.len();
            for (&ty, edges) in &graph {
                if edges.iter().any(|(child, _)| reaches.contains(child)) {
                    reaches.insert(ty);
                }
            }
            if reaches.len() == before {
                break;
            }
        }
        let mut reads = Reads {
            opaque: !opaque.is_empty(),
            ..Reads::default()
        };
        for edges in graph.values() {
            for (_, slot) in edges.iter().filter(|(child, _)| reaches.contains(child)) {
                match *slot {
                    Slot::Field(ty, index) => {
                        reads.fields.insert((ty, index));
                    }
                    Slot::Element(ty) => {
                        reads.elements.insert(ty);
                    }
                    Slot::Union(ty) => {
                        reads.unions.insert(ty);
                    }
                }
            }
        }
        reads
    }

    fn hit(&self, writes: &ModRef) -> bool {
        match writes {
            ModRef::Top => true,
            ModRef::Known(known) => {
                // Unrelated field/slot writes must not invalidate a modeled path. When the
                // receiver→result path could not be modeled (`opaque`), only wholesale slot
                // writes are treated as fatal — not every Known effect from an unrelated call.
                writes.hits(&self.fields, &self.unions)
                    || self.elements.iter().any(|&ty| writes.writes_element(ty))
                    || (self.opaque && known.any_slot)
            }
        }
    }
}

pub(super) struct Context<'a> {
    interner: &'a TypeInterner,
    layouts: &'a LayoutTable,
    table: &'a ModRefTable,
}

impl<'a> Context<'a> {
    pub(super) fn new(
        interner: &'a TypeInterner,
        layouts: &'a LayoutTable,
        table: &'a ModRefTable,
    ) -> Self {
        Self {
            interner,
            layouts,
            table,
        }
    }

    pub(super) fn invalidated(&self, f: &MirFunction, id: u32, base: u32) -> bool {
        let (i, layouts, table) = (self.interner, self.layouts, self.table);
        let reads = Reads::of(f.local_ty(Local(base)), f.local_ty(Local(id)), i, layouts);
        // Implicit destructor effects are checked on concrete `Release`/`ValueDrop` statements
        // below. A module-wide `del` join must not run here: one unrelated Top destructor in the
        // same MIR module would otherwise invalidate every borrowed getter in the function.
        let mut aliases = BTreeSet::from([id]);
        loop {
            let before = aliases.len();
            for stmt in f.blocks.iter().flat_map(|b| &b.stmts) {
                let Statement::Assign(Place::Local(dest), rv) = stmt else {
                    continue;
                };
                if !i.is_rc_tracked(f.local_ty(*dest)) && !i.is_value_type(f.local_ty(*dest)) {
                    continue;
                }
                let source = match rv {
                    Rvalue::Use(Operand::Copy(place)) | Rvalue::Cast(Operand::Copy(place), ..) => {
                        match place {
                            Place::Local(l)
                            | Place::Field { base: l, .. }
                            | Place::Index { base: l, .. } => Some(l.0),
                            _ => None,
                        }
                    }
                    Rvalue::UnionField {
                        base: Operand::Copy(Place::Local(l)),
                        ..
                    } => Some(l.0),
                    Rvalue::Call { callee, args } if callee.borrowed_result => match args.first() {
                        Some(Operand::Copy(Place::Local(base))) => Some(base.0),
                        _ => None,
                    },
                    _ => None,
                };
                if source.is_some_and(|source| aliases.contains(&source)) {
                    aliases.insert(dest.0);
                }
            }
            if aliases.len() == before {
                break;
            }
        }
        let live = liveness::live_out(f);
        for (bi, block) in f.blocks.iter().enumerate() {
            let mut at_statement = live[bi].clone();
            liveness::add_terminator_reads(&block.terminator, &mut at_statement);
            for stmt in block.stmts.iter().rev() {
                let after = aliases.iter().any(|alias| at_statement.contains(alias));
                liveness::transfer_stmt(stmt, &mut at_statement);
                let before = aliases.iter().any(|alias| at_statement.contains(alias));
                let defines_result =
                    matches!(stmt, Statement::Assign(Place::Local(dest), _) if dest.0 == id);
                // A fresh result is not exposed to the getter's own effects. A derived
                // alias from an earlier loop iteration can still cross this invocation.
                if !before && (defines_result || !after) {
                    continue;
                }
                if matches!(stmt, Statement::Assign(Place::Local(dest), _) | Statement::Release(Operand::Copy(Place::Local(dest))) if dest.0 == base)
                {
                    return true;
                }
                let mut hit = false;
                stmt_effects(stmt, f, i, |effect| {
                    hit |= match effect {
                        // ObserveOwnership is a refcount read (for example unique-move checks);
                        // it does not overwrite the borrowed path by itself.
                        Effect::Top | Effect::GlobalStore(_) => true,
                        Effect::ObserveOwnership => false,
                        Effect::Store(base, field) => {
                            reads.fields.contains(&(f.local_ty(base), field))
                                || reads.unions.contains(&f.local_ty(base))
                                || reads.opaque
                        }
                        Effect::SlotStore(ty) => reads.elements.contains(&ty) || reads.opaque,
                        Effect::Call(callee) => reads.hit(&table.call(callee)),
                        Effect::Iface(id, slot) => reads.hit(&table.iface(id, slot)),
                        Effect::Ctor(def) => reads.hit(&table.ctor(def)),
                        Effect::None => false,
                    };
                });
                // Do not consult the module-wide `del` join: one unrelated Top destructor would
                // make every `Release` look like it can rewrite this path.
                if hit {
                    return true;
                }
            }
            let mut at_terminator = live[bi].clone();
            liveness::add_terminator_reads(&block.terminator, &mut at_terminator);
            if aliases.iter().any(|alias| at_terminator.contains(alias)) {
                match &block.terminator {
                    Terminator::Await { .. } => return true,
                    Terminator::TailCall { callee, .. } if reads.hit(&table.call(callee)) => {
                        return true;
                    }
                    _ => {}
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::effects::Known;
    use dream_hir::TypeLayout;
    use dream_types::DefId;

    #[test]
    fn another_getter_invocation_can_invalidate_an_older_alias() {
        let mut i = TypeInterner::new();
        let array = i.array(i.string());
        let mut b = crate::build::FunctionBuilder::new("older_alias", i.int());
        let receiver = b.new_param(array, None);
        let value = b.new_local(i.string(), None);
        let alias = b.new_local(i.string(), None);
        let len = b.new_local(i.int(), None);
        b.assign(
            Place::Local(value),
            Rvalue::Use(Operand::Copy(Place::index(
                receiver,
                Operand::Const(crate::Const::Int(0)),
            ))),
        );
        b.assign(
            Place::Local(alias),
            Rvalue::Use(Operand::Copy(Place::Local(value))),
        );
        b.assign(
            Place::Local(value),
            Rvalue::Call {
                callee: crate::Callee {
                    def: DefId::root(703),
                    args: vec![],
                    ret: i.string(),
                    take_params: vec![false],
                    borrowed_result: true,
                },
                args: vec![Operand::Copy(Place::Local(receiver))],
            },
        );
        b.assign(
            Place::Local(len),
            Rvalue::StrLen(Operand::Copy(Place::Local(alias))),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(len)))));
        let f = b.finish();
        assert!(
            Context::new(&i, &LayoutTable::default(), &ModRefTable::default())
                .invalidated(&f, value.0, receiver.0)
        );
    }

    #[test]
    fn only_writes_during_the_derived_alias_lifetime_invalidate() {
        for after in [false, true] {
            let mut i = TypeInterner::new();
            let array = i.array(i.string());
            let mut b = crate::build::FunctionBuilder::new("snapshot", i.int());
            let receiver = b.new_param(array, None);
            let value = b.new_local(i.string(), None);
            let alias = b.new_local(i.string(), None);
            let len = b.new_local(i.int(), None);
            let write = Statement::Assign(
                Place::index(receiver, Operand::Const(crate::Const::Int(0))),
                Rvalue::Use(Operand::Const(crate::Const::Null)),
            );
            if !after {
                b.push(write.clone());
            }
            b.assign(
                Place::Local(value),
                Rvalue::Use(Operand::Copy(Place::index(
                    receiver,
                    Operand::Const(crate::Const::Int(0)),
                ))),
            );
            b.assign(
                Place::Local(alias),
                Rvalue::Use(Operand::Copy(Place::Local(value))),
            );
            if after {
                b.push(write);
            }
            b.assign(
                Place::Local(len),
                Rvalue::StrLen(Operand::Copy(Place::Local(alias))),
            );
            b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(len)))));
            let f = b.finish();
            let layouts = LayoutTable::default();
            let table = ModRefTable::default();
            assert_eq!(
                Context::new(&i, &layouts, &table).invalidated(&f, value.0, receiver.0),
                after
            );
        }
    }

    #[test]
    fn mutating_the_result_does_not_overwrite_its_container_slot() {
        let mut i = TypeInterner::new();
        let map = i.struct_ty(DefId::root(701), vec![]);
        let list = i.struct_ty(DefId::root(702), vec![]);
        let slots = i.array(map);
        let mut layouts = LayoutTable::default();
        layouts.insert(
            list,
            TypeLayout::from_fields(&i, "List", [("items".into(), slots, false, false)]),
        );
        let reads = Reads::of(list, map, &i, &layouts);
        assert!(!reads.hit(&ModRef::Known(Known {
            fields: [(map, 0)].into(),
            ..Known::default()
        })));
        assert!(reads.hit(&ModRef::Known(Known {
            fields: [(list, 0)].into(),
            ..Known::default()
        })));
        assert!(reads.hit(&ModRef::Known(Known {
            slots: [map].into(),
            ..Known::default()
        })));
        assert!(reads.hit(&ModRef::Top));
        assert!(!reads.hit(&ModRef::Known(Known {
            observes_rc: true,
            ..Known::default()
        })));
    }
}
