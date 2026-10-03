//! `sizeof(T)` and `nameof(path)` — compile-time meta forms (not reserved keywords).

use super::*;
use crate::errors::SemanticError;
use dream_diagnostics::DiagnosticBag;
use dream_hir::{HExpr, HExprKind};
use dream_syntax::nodes::Type;
use dream_syntax::token::syntax_token::SyntaxToken;

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
            self.ensure_struct_instantiated(&base_name, &generic_args, &pos, diagnostics);
        }

        let type_name = ty.get_type();
        if type_name == "void" || type_name.is_empty() {
            self.hir_none();
            report(
                diagnostics,
                "sizeof requires a complete type".to_string(),
                ty.get_span(),
            );
            return Ok(Type::Unknown);
        }

        if !self.sizeof_type_known(&type_name) {
            self.hir_none();
            report(
                diagnostics,
                format!("sizeof: unknown type '{}'", type_name),
                ty.get_span(),
            );
            return Ok(Type::Unknown);
        }

        let int_ty = Self::type_from_name("int");
        let ty_id = self.type_ctx.interner.int();
        let sized_ty = self.type_ctx.lower(ty);
        self.hir_set_last(Some(HExpr::new(ty_id, HExprKind::SizeOf(sized_ty))));
        Ok(int_ty)
    }

    fn sizeof_type_known(&self, type_name: &str) -> bool {
        if self.struct_info(type_name).is_some() {
            return true;
        }
        if type_name.ends_with("[]") {
            return true;
        }
        if type_name == "string"
            || type_name == "object"
            || type_name == "js"
            || type_name == "void"
        {
            return true;
        }
        if self.enum_members(type_name).is_some() {
            return true;
        }
        if self.interface_method_list(type_name).is_some() {
            return true;
        }
        if type_name.starts_with("fun(") {
            return true;
        }
        if type_name.starts_with("Future<") {
            return true;
        }
        if type_name.starts_with("Result<") {
            return true;
        }
        if type_name.starts_with("Option<") {
            return true;
        }
        matches!(
            type_name,
            "int"
                | "uint"
                | "float"
                | "char"
                | "byte"
                | "bool"
                | "long"
                | "ulong"
                | "double"
                | "isize"
                | "usize"
        )
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

        let type_name = operand_type.get_type();
        if type_name == "void" {
            self.hir_none();
            report(
                diagnostics,
                "typeof requires a value, got 'void'".to_string(),
                operand.position(),
            );
            return Ok(Type::Unknown);
        }

        let string_ty = Self::type_from_name("string");
        if type_name == "object" || self.is_interface_name(&type_name) {
            let value = self.hir_take();
            self.hir_set_type_name(value);
        } else {
            let display = self.ty_display(&operand_type);
            let ty_id = self.type_ctx.interner.string();
            self.hir_set_last(Some(HExpr::new(ty_id, HExprKind::StringLit(display))));
        }
        Ok(string_ty)
    }
}
