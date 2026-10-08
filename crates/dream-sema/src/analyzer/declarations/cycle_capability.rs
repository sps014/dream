//! Static strong-cycle permission checking; annotations do not alter runtime ownership.
use super::*;
use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};
use indexmap::IndexSet;

fn children(layouts: &LayoutTable, interner: &TypeInterner, ty: TypeId) -> Vec<TypeId> {
    if let Some(layout) = layouts.structs.get(&ty) {
        return layout
            .fields
            .iter()
            .filter(|f| !f.is_weak && !f.is_unowned)
            .map(|f| f.ty)
            .collect();
    }
    if let Some(layout) = layouts.unions.get(&ty) {
        return layout
            .variants
            .iter()
            .flat_map(|v| &v.fields)
            .filter(|f| !f.is_weak && !f.is_unowned)
            .map(|f| f.ty)
            .collect();
    }
    match interner.kind(ty) {
        TyKind::Array(elem) => vec![*elem],
        TyKind::Tuple(elems) => elems.clone(),
        TyKind::Union(_, args) if interner.is_niche_union(ty) => args.clone(),
        _ => Vec::new(),
    }
}

fn cycle_capable(layouts: &LayoutTable, interner: &TypeInterner, root: TypeId) -> bool {
    let mut pending = children(layouts, interner, root);
    let mut seen = IndexSet::new();
    while let Some(ty) = pending.pop() {
        if ty == root
            || matches!(
                interner.kind(ty),
                TyKind::Object | TyKind::Interface(..) | TyKind::Func(..)
            )
        {
            return true;
        }
        if interner.is_reference(ty)
            && match interner.kind(ty) {
                TyKind::Struct(..) => !layouts.structs.contains_key(&ty),
                TyKind::Union(..) => {
                    !interner.is_niche_union(ty) && !layouts.unions.contains_key(&ty)
                }
                _ => false,
            }
        {
            return true;
        }
        if seen.insert(ty) {
            pending.extend(children(layouts, interner, ty));
        }
    }
    false
}

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn validate_cycle_capability(
        &self,
        node: &'a ProgramView<'a>,
        layouts: &LayoutTable,
        diagnostics: &mut DiagnosticBag,
    ) {
        let previous_file = diagnostics.file_path.clone();
        for declaration in &node.structs {
            if declaration.is_value || dream_abi::attributes::allows_cycle(&declaration.attributes)
            {
                continue;
            }
            let scope = self.graph.module_for_file(declaration.file_path.as_deref());
            let Some(def) =
                self.type_ctx
                    .resolve_from(scope, DefKind::Struct, &declaration.name.text)
            else {
                continue;
            };
            // Each generic instantiation has its own concrete fields, but diagnoses belong to
            // the source declaration and are emitted once regardless of discovery order.
            let capable = layouts.structs.keys().any(|&ty| {
                matches!(self.type_ctx.interner.kind(ty), TyKind::Struct(d, _) if *d == def)
                    && cycle_capable(layouts, &self.type_ctx.interner, ty)
            });
            if capable {
                diagnostics.file_path = file_path_string(&declaration.file_path);
                diagnostics.report_error(
                    format!("class '{}' can form a strong ownership cycle; use weak/unowned back-links or add @allow_cycle to acknowledge possible leaks", declaration.name.text),
                    Some(declaration.name.position),
                );
            }
        }
        diagnostics.file_path = previous_file;
    }
}
