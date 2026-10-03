//! `sizeof(T)` and `nameof(path)` — compile-time meta forms (not reserved keywords).

use super::*;
use crate::errors::SemanticError;
use dream_diagnostics::DiagnosticBag;
use dream_hir::{HExpr, HExprKind};
use dream_syntax::nodes::Type;
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_types::{TyKind, TypeId};

impl<'a> Analyzer<'a> {
    /// Resolves `sizeof(T)` to a type identity; MIR folds it using the completed target layout.
    pub(in crate::analyzer) fn analyze_sizeof(
        &mut self,
        ty: &Type,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        // Instantiate generic targets so struct-table keys exist.
        let mut core = ty;
        while let Type::Array(inner) = core {
            core = inner;
        }
        if let Some((base_name, generic_args)) = Self::resolve_struct_parts(core) {
            let pos = ty.get_span().unwrap_or_else(empty_span);
            let core_id = self.type_ctx.lower(core);
            match self.type_ctx.interner.kind(core_id) {
                TyKind::Struct(..) => {
                    self.ensure_struct_instantiated(&base_name, &generic_args, &pos, diagnostics)
                }
                TyKind::Union(..) => {
                    self.ensure_union_instantiated(&base_name, &generic_args, &pos, diagnostics)
                }
                TyKind::Interface(..) => {
                    self.ensure_interface_instantiated(&base_name, &generic_args, &pos, diagnostics)
                }
                _ => {}
            }
        }

        let sized_ty = self.type_ctx.lower(ty);
        if matches!(self.type_ctx.interner.kind(sized_ty), TyKind::Void) {
            self.hir_none();
            report(
                diagnostics,
                "sizeof requires a complete type".to_string(),
                ty.get_span(),
            );
            return Ok(Type::Unknown);
        }

        if !self.sizeof_type_known(sized_ty) {
            self.hir_none();
            report(
                diagnostics,
                format!("sizeof: unknown type '{}'", ty.display_name()),
                ty.get_span(),
            );
            return Ok(Type::Unknown);
        }

        let int_ty = Self::type_from_name("int");
        let ty_id = self.type_ctx.interner.int();
        self.hir_set_last(Some(HExpr::new(ty_id, HExprKind::SizeOf(sized_ty))));
        Ok(int_ty)
    }

    fn sizeof_type_known(&self, ty: TypeId) -> bool {
        match self.type_ctx.interner.kind(ty) {
            TyKind::Struct(..) => self.struct_info(ty).is_some(),
            TyKind::Union(..) => self.union_info(ty).is_some(),
            TyKind::Enum(def) => self.enum_members(*def).is_some(),
            TyKind::Interface(..) => self.interface_method_list(ty).is_some(),
            TyKind::Tuple(elements) => elements.iter().all(|&elem| self.sizeof_type_known(elem)),
            TyKind::Prim(_) | TyKind::Object | TyKind::Js | TyKind::Array(_) | TyKind::Func(..) => {
                true
            }
            TyKind::Void | TyKind::Error => false,
        }
    }

    /// `nameof(a.b.c)` → string literal of the last path segment. Operand is not evaluated.
    pub(in crate::analyzer) fn analyze_nameof(
        &mut self,
        parts: &[SyntaxToken],
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        if parts.is_empty() {
            self.hir_none();
            let _ = report(diagnostics, "nameof requires a name".to_string(), None);
            return Ok(Type::Unknown);
        }
        let Some(last) = parts.last() else {
            crate::internal_error!("non-empty nameof path lost its final segment");
        };
        let name = last.text.clone();
        let string_ty = Self::type_from_name("string");
        let ty_id = self.type_ctx.interner.string();
        self.hir_set_last(Some(HExpr::new(ty_id, HExprKind::StringLit(name))));
        Ok(string_ty)
    }

    /// `typeof(expr)` → `string` naming the operand's concrete type.
    ///
    /// An `object` or interface operand resolves at runtime from the value's heap tag, so a boxed
    /// dynamic reports what it actually holds and an interface slot reports the implementing class.
    /// Every other operand's static type already *is* its concrete type (Dream has no class
    /// inheritance), so it folds to a string literal and — like `nameof` — is not evaluated.
    pub(in crate::analyzer) fn analyze_typeof(
        &mut self,
        operand: &ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let operand_type =
            self.analyze_expression(operand, parent_function, symbol_table, diagnostics)?;
        if operand_type.is_unknown() {
            self.hir_none();
            return Ok(Type::Unknown);
        }

        let value = self.hir_take();
        let operand_id = value
            .as_ref()
            .map(|hir| hir.ty)
            .unwrap_or_else(|| self.type_ctx.lower(&operand_type));
        if matches!(self.type_ctx.interner.kind(operand_id), TyKind::Void) {
            self.hir_none();
            report(
                diagnostics,
                "typeof requires a value, got 'void'".to_string(),
                operand.position(),
            );
            return Ok(Type::Unknown);
        }

        let string_ty = Self::type_from_name("string");
        if matches!(self.type_ctx.interner.kind(operand_id), TyKind::Object)
            || self.is_interface_name(operand_id)
        {
            self.hir_set_type_name(value);
        } else {
            let display = self.ty_display(&operand_type);
            let ty_id = self.type_ctx.interner.string();
            self.hir_set_last(Some(HExpr::new(ty_id, HExprKind::StringLit(display))));
        }
        Ok(string_ty)
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
    fn sizeof_enum_lookup_uses_definition_identity() {
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
        analyzer
            .type_ctx
            .define_module(ModuleId(1), "left".into(), vec![]);
        analyzer.type_ctx.set_scope(ModuleId(1));
        let left = analyzer.type_ctx.register(DefKind::Enum, "Color", vec![]);
        analyzer.enum_table.insert(left, indexmap::IndexMap::new());
        let left_ty = analyzer.type_ctx.instantiate(left, vec![]);
        analyzer
            .type_ctx
            .define_module(ModuleId(2), "right".into(), vec![]);
        analyzer.type_ctx.set_scope(ModuleId(2));
        let right = analyzer.type_ctx.register(DefKind::Enum, "Color", vec![]);
        let right_ty = analyzer.type_ctx.instantiate(right, vec![]);
        assert!(analyzer.sizeof_type_known(left_ty));
        assert!(!analyzer.sizeof_type_known(right_ty));
    }

    #[test]
    fn sizeof_rejects_poison_and_incomplete_tuple_members() {
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
        let int = analyzer.type_ctx.interner.int();
        let error = analyzer.type_ctx.interner.error();
        let valid = analyzer.type_ctx.interner.tuple_ty(vec![int, int]);
        let invalid = analyzer.type_ctx.interner.tuple_ty(vec![int, error]);
        assert!(analyzer.sizeof_type_known(valid));
        assert!(!analyzer.sizeof_type_known(invalid));
        assert!(!analyzer.sizeof_type_known(error));
        assert!(!analyzer.sizeof_type_known(analyzer.type_ctx.interner.void()));
    }
}
