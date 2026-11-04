//! Use-after-move checking for unique values. A unique value has one owner: passing it to a
//! parameter that keeps it, binding or assigning it, or placing it into an aggregate moves it,
//! and the source local cannot be used again until it is reassigned. The check runs on the
//! resolved HIR after parameter inference, so a call moves exactly the arguments its callee's
//! body keeps (not every argument of the sink calling convention). Locals introduced by
//! desugaring, `this`, `ref` parameters and capture cells are not tracked.

mod report;
mod state;
mod walk;

use super::*;
use dream_abi::names::THIS_NAME;
use dream_hir::{HFunction, LayoutTable, LocalId, ParamMode};
use dream_types::{DefId, DefKind, TypeId};
use indexmap::{IndexMap, IndexSet};
use walk::{Finding, Walker};

/// Locals the compiler introduces while desugaring, such as a `for` loop's sequence. Binding one
/// borrows its value for the desugared construct instead of moving it.
pub(in crate::analyzer) fn is_synthetic(name: &str) -> bool {
    name.starts_with("__") || name.starts_with('$')
}

/// Parameters (except `ref` ones) and locals of `function`, with their names and types.
fn bindings(function: &HFunction) -> IndexMap<LocalId, (&str, TypeId)> {
    function
        .params
        .iter()
        .filter(|p| p.mode != ParamMode::Ref)
        .map(|p| (p.local, (p.name.as_str(), p.ty)))
        .chain(
            function
                .locals
                .iter()
                .map(|l| (l.id, (l.name.as_str(), l.ty))),
        )
        .collect()
}

/// What the walker needs to describe where a value went.
struct Context<'c> {
    functions: &'c IndexMap<DefId, (String, Vec<String>)>,
    /// `(function, parameter)` pairs whose body keeps the argument.
    kept: &'c IndexSet<(DefId, usize)>,
    layouts: &'c LayoutTable,
    types: &'c dream_types::TypeCtx,
    bindings: &'c IndexMap<LocalId, (&'c str, TypeId)>,
    /// Indexer setters: their value argument lands in `receiver[key]`.
    setters: &'c IndexSet<DefId>,
    /// The storage of `weak` locals, which observes its target without owning it.
    weak_box: Option<DefId>,
}

impl Context<'_> {
    fn local_name(&self, local: LocalId) -> String {
        self.bindings
            .get(&local)
            .map(|(name, _)| name.to_string())
            .unwrap_or_default()
    }

    fn function(&self, def: DefId) -> (String, &[String]) {
        match self.functions.get(&def) {
            Some((name, params)) => (name.clone(), params.as_slice()),
            None => (String::new(), &[]),
        }
    }

    fn type_name(&self, ty: TypeId) -> String {
        dream_types::display_name(&self.types.interner, &self.types.defs, ty)
    }

    fn field_name(&self, ty: TypeId, field: usize) -> Option<String> {
        Some(self.layouts.get(ty)?.fields.get(field)?.name.clone())
    }

    fn is_weak_field(&self, ty: TypeId, field: usize) -> bool {
        self.layouts
            .get(ty)
            .and_then(|layout| layout.fields.get(field))
            .is_some_and(|f| f.is_weak || f.is_unowned)
    }

    /// Source spelling of a simple place (`this.items`, `node.next`, `slots[i]`).
    fn place_text(&self, expr: &dream_hir::HExpr) -> Option<String> {
        use dream_hir::{Binding, HExprKind};
        match &expr.kind {
            HExprKind::Var(Binding::Local(local)) => {
                let name = self.local_name(*local);
                (!name.is_empty() && !is_synthetic(&name)).then_some(name)
            }
            HExprKind::Field { obj, field } => Some(format!(
                "{}.{}",
                self.place_text(obj)?,
                self.field_name(obj.ty, *field)?
            )),
            HExprKind::IntLit(value) => Some(value.to_string()),
            HExprKind::StringLit(value) => Some(format!("{value:?}")),
            _ => None,
        }
    }
}

/// Every use of a moved local in one function.
struct FunctionFindings<'f> {
    function: &'f HFunction,
    findings: Vec<Finding>,
}

impl<'a> Analyzer<'a> {
    pub(super) fn check_moves(
        &self,
        functions: &[HFunction],
        kept: &IndexSet<(DefId, usize)>,
        layouts: &LayoutTable,
        diagnostics: &mut DiagnosticBag,
    ) {
        let interner = &self.type_ctx.interner;
        let capture_cell = self.type_ctx.resolve(DefKind::Struct, "CaptureCell");
        let weak_box = self.type_ctx.resolve(DefKind::Struct, "system.core::Weak");
        let mut methods: IndexMap<DefId, (TypeId, &str)> = IndexMap::new();
        for ((owner, member), identities) in &self.function_table.methods {
            for (def, _) in identities {
                methods.entry(*def).or_insert((*owner, member));
            }
        }
        let setters: IndexSet<DefId> = methods
            .iter()
            .filter(|(_, (_, member))| *member == dream_syntax::nodes::types::SET_INDEXER_NAME)
            .map(|(def, _)| *def)
            .collect();
        let signatures: IndexMap<DefId, (String, Vec<String>)> = functions
            .iter()
            .map(|f| {
                let params = f.params.iter().map(|p| p.name.clone()).collect();
                let name = match methods.get(&f.def) {
                    Some(&(owner, member)) => format!(
                        "{}.{member}",
                        dream_types::display_name(interner, &self.type_ctx.defs, owner)
                    ),
                    None => self
                        .function_table
                        .functions
                        .get(&(f.def, f.instance.clone()))
                        .map(|info| info.name.clone())
                        .unwrap_or_else(|| self.type_ctx.defs.get(f.def).name.clone()),
                };
                (f.def, (name, params))
            })
            .collect();
        let mut found = Vec::new();
        for function in functions {
            let bindings = bindings(function);
            let tracked: IndexSet<LocalId> = bindings
                .iter()
                .filter(|(_, (name, ty))| {
                    !is_synthetic(name)
                        && *name != THIS_NAME
                        && interner.is_unique_type(*ty)
                        && !matches!(interner.kind(*ty), dream_types::TyKind::Struct(def, _)
                            if Some(*def) == capture_cell)
                })
                .map(|(local, _)| *local)
                .collect();
            if tracked.is_empty() {
                continue;
            }
            let cx = Context {
                functions: &signatures,
                kept,
                layouts,
                types: &self.type_ctx,
                bindings: &bindings,
                setters: &setters,
                weak_box,
            };
            let mut walker = Walker::new(&cx, &tracked);
            walker.stmts(&function.body);
            if !walker.findings.is_empty() {
                found.push(FunctionFindings {
                    function,
                    findings: walker.findings,
                });
            }
        }
        self.report_moves(found, diagnostics);
    }
}
