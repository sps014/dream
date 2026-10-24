//! Strong type graph: which stores could possibly close a cycle, judged from layouts alone.

use super::value::Slot;
use dream_hir::{InterfaceTable, LayoutTable};
use dream_types::{PrimTy, TyKind, TypeId, TypeInterner};
use indexmap::IndexMap;
use petgraph::graph::{DiGraph, NodeIndex};

/// How a strong edge leaves its owner, for naming the field on a reported cycle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Edge {
    Field(TypeId, u32),
    Payload(TypeId, u32, u32),
    Elem,
    Inline,
    Erased,
}

const NONE: u32 = u32::MAX;

pub(super) struct TypeGraph<'a> {
    interner: &'a TypeInterner,
    layouts: &'a LayoutTable,
    implementers: IndexMap<TypeId, Vec<TypeId>>,
    references: Vec<TypeId>,
    scc: Vec<u32>,
    cyclic: Vec<bool>,
    bearing: Vec<bool>,
    pub(super) any_cyclic: bool,
}

impl<'a> TypeGraph<'a> {
    pub(super) fn build(
        interner: &'a TypeInterner,
        layouts: &'a LayoutTable,
        interfaces: &InterfaceTable,
    ) -> Self {
        let mut implementers: IndexMap<TypeId, Vec<TypeId>> = IndexMap::new();
        for imp in &interfaces.impls {
            for (iface, _) in &imp.entries {
                if let Some(info) = interfaces.interfaces.get(*iface) {
                    implementers.entry(info.ty).or_default().push(imp.class_ty);
                }
            }
        }
        let references = interner
            .iter_kinds()
            .filter(|(id, kind)| {
                interner.is_reference(*id) && !matches!(kind, TyKind::Prim(PrimTy::String))
            })
            .map(|(id, _)| id)
            .collect();
        let mut graph = TypeGraph {
            interner,
            layouts,
            implementers,
            references,
            scc: Vec::new(),
            cyclic: Vec::new(),
            bearing: Vec::new(),
            any_cyclic: false,
        };
        graph.compute();
        graph
    }

    fn compute(&mut self) {
        let count = self.interner.len();
        let mut g: DiGraph<(), ()> = DiGraph::with_capacity(count, count);
        for _ in 0..count {
            g.add_node(());
        }
        let mut self_loop = vec![false; count];
        for (index, has_self_loop) in self_loop.iter_mut().enumerate() {
            let ty = TypeId(index as u32);
            for (child, _) in self.children(ty) {
                *has_self_loop |= child == ty;
                g.add_edge(NodeIndex::new(index), NodeIndex::new(child.0 as usize), ());
            }
        }
        // Tarjan yields components in reverse topological order: successors come first.
        let components = petgraph::algo::tarjan_scc(&g);
        self.scc = vec![NONE; count];
        self.cyclic = Vec::with_capacity(components.len());
        let mut scc_bearing = Vec::with_capacity(components.len());
        for (id, members) in components.iter().enumerate() {
            for node in members {
                self.scc[node.index()] = id as u32;
            }
            let cyclic = members.len() > 1 || self_loop[members[0].index()];
            self.cyclic.push(cyclic);
            self.any_cyclic |= cyclic;
            let reaches = members.iter().any(|node| {
                g.neighbors(*node).any(|next| {
                    let other = self.scc[next.index()];
                    other != id as u32 && other != NONE && scc_bearing[other as usize]
                })
            });
            scc_bearing.push(cyclic || reaches);
        }
        self.bearing = self
            .scc
            .iter()
            .map(|&scc| scc != NONE && scc_bearing[scc as usize])
            .collect();
    }

    /// Strong children of `ty`. Erased `object`, closure and interface values may hold any
    /// reference their runtime referent could, so they fan out conservatively.
    pub(super) fn children(&self, ty: TypeId) -> Vec<(TypeId, Edge)> {
        let strong = |weak: bool, unowned: bool| !weak && !unowned;
        if let Some(layout) = self.layouts.structs.get(&ty) {
            return layout
                .fields
                .iter()
                .enumerate()
                .filter(|(_, f)| strong(f.is_weak, f.is_unowned))
                .map(|(i, f)| (f.ty, Edge::Field(ty, i as u32)))
                .collect();
        }
        if let Some(layout) = self.layouts.unions.get(&ty) {
            return layout
                .variants
                .iter()
                .enumerate()
                .flat_map(|(v, variant)| {
                    variant
                        .fields
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| strong(f.is_weak, f.is_unowned))
                        .map(move |(i, f)| (f.ty, Edge::Payload(ty, v as u32, i as u32)))
                })
                .collect();
        }
        let erased = || vec![(self.interner.object(), Edge::Erased)];
        match self.interner.kind(ty) {
            TyKind::Array(elem) => vec![(*elem, Edge::Elem)],
            TyKind::Tuple(elems) => elems.iter().map(|e| (*e, Edge::Inline)).collect(),
            TyKind::Union(_, args) if self.interner.is_niche_union(ty) => {
                args.iter().map(|a| (*a, Edge::Inline)).collect()
            }
            TyKind::Union(..) | TyKind::Struct(..) if self.interner.is_reference(ty) => erased(),
            TyKind::Object => self
                .references
                .iter()
                .map(|r| (*r, Edge::Erased))
                .collect(),
            TyKind::Interface(..) => match self.implementers.get(&ty) {
                Some(classes) => classes.iter().map(|c| (*c, Edge::Erased)).collect(),
                None => erased(),
            },
            TyKind::Func(..) => erased(),
            _ => Vec::new(),
        }
    }

    fn scc_of(&self, ty: TypeId) -> u32 {
        self.scc.get(ty.0 as usize).copied().unwrap_or(NONE)
    }

    /// True when values of `ty` can reach a type that participates in a strong type cycle.
    pub(super) fn bearing(&self, ty: TypeId) -> bool {
        self.bearing.get(ty.0 as usize).copied().unwrap_or(false)
    }

    /// A store of `value` into `container` can only close a cycle when `value` can reach
    /// `container` again, i.e. both sit in the same cyclic component.
    pub(super) fn relevant(&self, container: TypeId, value: TypeId) -> bool {
        let scc = self.scc_of(container);
        scc != NONE && scc == self.scc_of(value) && self.cyclic[scc as usize]
    }

    /// The declared type of what `slot` holds.
    pub(super) fn slot_type(&self, slot: Slot) -> Option<TypeId> {
        match slot {
            Slot::Field(owner, index) | Slot::Weak(owner, index) => self
                .layouts
                .structs
                .get(&owner)
                .and_then(|layout| layout.fields.get(index as usize))
                .map(|field| field.ty),
            Slot::Payload(owner, variant, index) => self
                .layouts
                .unions
                .get(&owner)
                .and_then(|layout| layout.variants.get(variant as usize))
                .and_then(|variant| variant.fields.get(index as usize))
                .map(|field| field.ty),
            Slot::Elem(array) => match self.interner.kind(array) {
                TyKind::Array(elem) => Some(*elem),
                _ => None,
            },
        }
    }

    /// Whether an object allocated as one of `types` can contain `slot`: its owner's own objects,
    /// or for an inline value owner, objects that embed that value.
    pub(super) fn owns(&self, types: &[TypeId], slot: Slot) -> bool {
        let owner = match slot {
            Slot::Field(ty, _) | Slot::Weak(ty, _) | Slot::Payload(ty, ..) | Slot::Elem(ty) => ty,
        };
        let inline = self.interner.is_value_type(owner);
        types
            .iter()
            .any(|&ty| ty == owner || (inline && self.embeds(ty, owner)))
    }

    /// Whether `holder` stores a `value` inline (not behind a reference). Value types cannot
    /// embed themselves, so the walk is finite. A reference object without a layout (a future
    /// or a host object) has no field a store could name.
    fn embeds(&self, holder: TypeId, value: TypeId) -> bool {
        if holder == value {
            return true;
        }
        let inline = |ty: TypeId| self.interner.is_value_type(ty) && self.embeds(ty, value);
        match self.interner.kind(holder) {
            TyKind::Array(elem) => inline(*elem),
            TyKind::Struct(..) => match self.layouts.structs.get(&holder) {
                Some(layout) => layout.fields.iter().any(|field| inline(field.ty)),
                None => !self.interner.is_reference(holder),
            },
            TyKind::Union(..) => self.layouts.unions.get(&holder).is_none_or(|layout| {
                layout
                    .variants
                    .iter()
                    .any(|variant| variant.fields.iter().any(|field| inline(field.ty)))
            }),
            TyKind::Tuple(elems) => elems.iter().any(|&elem| inline(elem)),
            _ => true,
        }
    }

    /// Whether a slot declared as `declared` can hold an object allocated as `allocated`.
    /// Classes have no subtypes and arrays are invariant, so those hold only their own type;
    /// erased types may hold anything.
    pub(super) fn may_hold(&self, declared: TypeId, allocated: TypeId) -> bool {
        if declared == allocated {
            return true;
        }
        let concrete = match self.interner.kind(allocated) {
            TyKind::Array(_) => true,
            TyKind::Struct(..) => self.interner.is_reference(allocated),
            _ => false,
        };
        if !concrete {
            return true;
        }
        match self.interner.kind(declared) {
            TyKind::Union(_, args) if self.interner.is_niche_union(declared) => {
                args.iter().any(|&arg| self.may_hold(arg, allocated))
            }
            // A boxed union holds only its box.
            TyKind::Array(_) | TyKind::Struct(..) | TyKind::Union(..)
                if self.interner.is_reference(declared) =>
            {
                false
            }
            // Inline values hold what their fields hold.
            TyKind::Struct(..) => self.layouts.structs.get(&declared).is_none_or(|layout| {
                layout
                    .fields
                    .iter()
                    .any(|field| self.may_hold(field.ty, allocated))
            }),
            TyKind::Union(..) => self.layouts.unions.get(&declared).is_none_or(|layout| {
                layout.variants.iter().any(|variant| {
                    variant
                        .fields
                        .iter()
                        .any(|field| self.may_hold(field.ty, allocated))
                })
            }),
            TyKind::Tuple(elems) => elems.iter().any(|&elem| self.may_hold(elem, allocated)),
            TyKind::Prim(_) => false,
            _ => true,
        }
    }

    pub(super) fn same_component(&self, a: TypeId, b: TypeId) -> bool {
        let scc = self.scc_of(a);
        scc != NONE && scc == self.scc_of(b)
    }

    pub(super) fn interner(&self) -> &'a TypeInterner {
        self.interner
    }

    pub(super) fn layouts(&self) -> &'a LayoutTable {
        self.layouts
    }
}
