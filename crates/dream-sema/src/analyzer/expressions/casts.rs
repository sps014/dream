//! `expr as T` cast validation and the `compare_data_type` assignability check that backs
//! assignments, argument passing, and comparisons.

use super::*;
use crate::errors::SemanticError;
use crate::symbol_table::SymbolTable;
use dream_diagnostics::DiagnosticBag;
use dream_hir::HExpr;
use dream_syntax::nodes::{ExpressionNode, FunctionNode, Type};
use dream_text::text_span::TextSpan;
use dream_types::{TyKind, TypeId};
use std::cell::RefCell;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    /// Types a cast `expr as T`: instantiates a generic target struct if needed, then validates the
    /// conversion (identity, numeric<->numeric, `char`<->`int`/`byte`, boxing/unboxing via `object`).
    /// Always yields the target type, reporting an error for disallowed conversions so analysis can
    /// continue.
    pub(in crate::analyzer) fn analyze_cast(
        &mut self,
        target_type: &Type,
        expr: &ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        // Cast operands must not inherit an outer expected type (e.g. assignment-to-double), or
        // integer literals inside `(double)((int)c - 48)` get retargeted incorrectly.
        let saved_expected = self.current_expected_type.take();
        let expr_type =
            self.analyze_expression(expr, parent_function, symbol_table, diagnostics)?;
        self.current_expected_type = saved_expected;
        self.check_type_not_static_class(target_type, diagnostics);
        let inner_hir = self.hir_take();
        let expr_id = inner_hir
            .as_ref()
            .map(|hir| hir.ty)
            .unwrap_or_else(|| self.type_ctx.lower(&expr_type));

        if expr_type.is_unknown() || target_type.is_unknown() {
            self.hir_none();
            return Ok(Type::Unknown);
        }

        // User-defined explicit conversion: `@cast("explicit")` (or `@cast("implicit")`, since an
        // explicit cast may always invoke an implicit one) on `expr`'s type converting to
        // `target_type`. Checked before the built-in conversion rules below so a struct's overload
        // always wins over (what would otherwise be) a "cannot cast" error.
        if let Some(cast) = self.operator_cast_fn(&expr_type, target_type, false) {
            self.hir_set_method_call(inner_hir, &cast.identity, vec![], target_type);
            return Ok(target_type.clone());
        }

        // If the target (after peeling array wrappers) is a generic struct, instantiate it.
        let mut core_target = target_type;
        while let Type::Array(inner) = core_target {
            core_target = inner;
        }
        if let Some((base_name, generic_args)) = Self::resolve_struct_parts(core_target) {
            self.ensure_struct_instantiated(&base_name, &generic_args, &empty_span(), diagnostics);
        }

        // The cast yields `target_type` regardless of whether the conversion is allowed (a
        // disallowed one is reported below); record its HIR before the validation branches.
        self.hir_set_cast(inner_hir, target_type);
        let target_id = self.type_ctx.lower(target_type);
        let numeric = |ty: &Type| ty.is_integer() || matches!(ty, Type::Float(_) | Type::Double(_));

        if target_id == expr_id ||
           (numeric(target_type) && numeric(&expr_type)) ||
           // `char` is a code point: allow lossless conversion to/from `int`/`byte`.
           (matches!(target_type, Type::Char(_)) && matches!(expr_type, Type::Integer(_) | Type::Byte(_))) ||
           (matches!(target_type, Type::Integer(_) | Type::Byte(_)) && matches!(expr_type, Type::Char(_)))
        {
            Ok(target_type.clone())
        } else if target_type.is_object() || expr_type.is_object() {
            // Boxing (`T as object`) and unboxing (`object as T`) are always permitted;
            // an unbox to the wrong primitive traps at runtime.
            Ok(target_type.clone())
        } else if expr_type.is_int()
            && (self.struct_info(target_id).is_some() || target_type.is_array())
        {
            // Allow casting int to pointer types (for null pointers)
            Ok(target_type.clone())
        } else if self.is_interface_name(target_id) {
            // Cast to an interface (`(Animal)cat`). Allowed from another interface, or a class that
            // implements the interface (an upcast). Both are identity at runtime (same tagged
            // pointer); only the static type changes.
            if self.is_interface_name(expr_id)
                || self.implements_as_interface_ref(expr_id, target_id, diagnostics)
            {
                Ok(target_type.clone())
            } else {
                diagnostics.report_error(
                    format!(
                        "Cannot cast from {} to interface {} ({} does not implement it)",
                        self.ty_display(&expr_type),
                        self.ty_display(target_type),
                        self.ty_display(&expr_type)
                    ),
                    target_type.get_span().or_else(|| expr.position()),
                );
                Ok(target_type.clone())
            }
        } else if self.is_interface_name(expr_id) {
            // Downcast from an interface to a concrete class or another interface: permitted
            // (identity at runtime; like unboxing `object`, a wrong downcast is the caller's risk).
            Ok(target_type.clone())
        } else {
            diagnostics.report_error(
                format!(
                    "Cannot cast from {} to {}",
                    self.ty_display(&expr_type),
                    self.ty_display(target_type)
                ),
                target_type.get_span().or_else(|| expr.position()),
            );
            Ok(target_type.clone())
        }
    }

    pub(in crate::analyzer) fn compare_data_type(
        &mut self,
        left: &Type,
        right: &Type,
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        // A poison operand (from an earlier reported error) is compatible with anything, so we
        // never emit a follow-on mismatch for it.
        if left.is_unknown() || right.is_unknown() {
            return Ok(());
        }

        // Directional assignability: `right` (value) must be assignable to `left` (target),
        // including class → interface and covariant `Result`/`Option` type arguments.
        let l = self.type_ctx.lower(left);
        let r = self.type_ctx.lower(right);
        if self.value_type_assignable(l, r, diagnostics) {
            return Ok(());
        }

        diagnostics.report_error(
            format!(
                "cannot convert from {} to {}",
                self.ty_display(right),
                self.ty_display(left),
            ),
            Some(*position),
        );
        Ok(())
    }

    /// Directional assignability: interned rules, class → interface, then covariant `Result` /
    /// `Option` when each type argument is assignable and layout-compatible.
    pub(in crate::analyzer) fn value_type_assignable(
        &mut self,
        target: TypeId,
        value: TypeId,
        diagnostics: &mut DiagnosticBag,
    ) -> bool {
        if dream_types::assignable(&self.type_ctx.interner, target, value) {
            return true;
        }
        if self.type_id_assignable_to_interface(target, value, diagnostics) {
            return true;
        }
        self.covariant_option_result(target, value, diagnostics)
    }

    pub(in crate::analyzer) fn type_id_assignable_to_interface(
        &mut self,
        target: TypeId,
        value: TypeId,
        diagnostics: &mut DiagnosticBag,
    ) -> bool {
        if !matches!(self.type_ctx.interner.kind(target), TyKind::Interface(..)) {
            return false;
        }
        self.implements_as_interface_ref(value, target, diagnostics)
    }

    /// `Result`/`Option` are covariant in their type arguments when a bitcast of the heap box is
    /// sound: each changed argument is either identical or both RC heap references (class →
    /// interface). A value-struct payload vs an interface pointer is rejected here; `?` still
    /// accepts that case because it rebuilds the wrapper.
    fn covariant_option_result(
        &mut self,
        target: TypeId,
        value: TypeId,
        diagnostics: &mut DiagnosticBag,
    ) -> bool {
        let (def, t_args, v_args) = match (
            self.type_ctx.interner.kind(target),
            self.type_ctx.interner.kind(value),
        ) {
            (TyKind::Union(td, ta), TyKind::Union(vd, va)) if td == vd => {
                (*td, ta.clone(), va.clone())
            }
            _ => return false,
        };
        let name = self.type_ctx.defs.name(def);
        if name != "Result" && name != "Option" {
            return false;
        }
        if t_args.len() != v_args.len() {
            return false;
        }
        for (t, v) in t_args.iter().zip(v_args.iter()) {
            if !self.payload_layout_compatible(*t, *v) {
                return false;
            }
            if !self.value_type_assignable(*t, *v, diagnostics) {
                return false;
            }
        }
        true
    }

    fn payload_layout_compatible(&self, target: TypeId, value: TypeId) -> bool {
        target == value
            || (self.type_ctx.interner.is_reference(target)
                && self.type_ctx.interner.is_reference(value))
    }

    /// If `value` (of static type `from`) has a registered `@cast("implicit")` method converting
    /// `from` to `to`, rewrites the HIR into a call to it and returns `to`; otherwise returns
    /// `from`/`value` unchanged. Meant to run just before a [`Self::compare_data_type`] check at a
    /// binding site (`let x: T = expr;`), so a user-defined implicit conversion is accepted there
    /// exactly like the built-in ones (numeric widening, boxing) instead of being rejected as a type
    /// mismatch.
    pub(in crate::analyzer) fn apply_implicit_cast(
        &mut self,
        from: &Type,
        to: &Type,
        value: Option<HExpr>,
    ) -> (Type, Option<HExpr>) {
        if self.type_ctx.lower(from) == self.type_ctx.lower(to) {
            return (from.clone(), value);
        }
        let Some(cast) = self.operator_cast_fn(from, to, true) else {
            return (from.clone(), value);
        };
        self.hir_set_method_call(value, &cast.identity, vec![], to);
        (to.clone(), self.hir_take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module_graph::ModuleGraph;
    use bumpalo::Bump;
    use dream_syntax::nodes::ProgramNode;
    use dream_types::{DefKind, ModuleId};

    #[test]
    fn assignability_keeps_same_named_module_types_distinct() {
        let arena = Bump::new();
        let graph = ModuleGraph::single(ProgramNode::new(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        ));
        let mut analyzer = Analyzer::new(&graph, &arena);
        let mut types = Vec::new();
        for (module, path) in [(ModuleId(1), "left"), (ModuleId(2), "right")] {
            analyzer.type_ctx.define_module(module, path.into(), vec![]);
            analyzer.type_ctx.set_scope(module);
            let def = analyzer.type_ctx.register(DefKind::Struct, "User", vec![]);
            let ty = analyzer.type_ctx.instantiate(def, vec![]);
            types.push(analyzer.type_ctx.syntax_type(ty));
        }
        let mut diagnostics = DiagnosticBag::new(None);
        analyzer
            .compare_data_type(&types[0], &types[1], &empty_span(), &mut diagnostics)
            .unwrap();
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn assignability_preserves_nested_generic_arguments() {
        let arena = Bump::new();
        let graph = ModuleGraph::single(ProgramNode::new(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        ));
        let mut analyzer = Analyzer::new(&graph, &arena);
        let box_def = analyzer
            .type_ctx
            .register(DefKind::Struct, "Box", vec!["T".into()]);
        let nested_def = analyzer
            .type_ctx
            .register(DefKind::Struct, "A", vec!["T".into()]);
        let flat_def = analyzer.type_ctx.register(DefKind::Struct, "A_B", vec![]);
        let leaf_def = analyzer.type_ctx.register(DefKind::Struct, "B", vec![]);
        let flat = analyzer.type_ctx.instantiate(flat_def, vec![]);
        let leaf = analyzer.type_ctx.instantiate(leaf_def, vec![]);
        let nested = analyzer.type_ctx.instantiate(nested_def, vec![leaf]);
        let left = analyzer.type_ctx.instantiate(box_def, vec![flat]);
        let right = analyzer.type_ctx.instantiate(box_def, vec![nested]);
        let left = analyzer.type_ctx.syntax_type(left);
        let right = analyzer.type_ctx.syntax_type(right);
        let mut diagnostics = DiagnosticBag::new(None);
        analyzer
            .compare_data_type(&left, &right, &empty_span(), &mut diagnostics)
            .unwrap();
        assert!(diagnostics.has_errors());
    }
}
