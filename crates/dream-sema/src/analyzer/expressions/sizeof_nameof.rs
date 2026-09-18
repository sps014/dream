//! `sizeof(T)` and `nameof(path)` — compile-time meta forms (not reserved keywords).

use super::*;
use crate::errors::SemanticError;
use dream_diagnostics::DiagnosticBag;
use dream_hir::{HExpr, HExprKind};
use dream_syntax::nodes::Type;
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_types::value_size_align;

impl<'a> Analyzer<'a> {
    /// `sizeof(T)` → `int` literal of Dream ABI storage size (refs/classes/arrays = 4).
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

        let size = match self.sizeof_bytes(&type_name) {
            Some(s) => s,
            None => {
                self.hir_none();
                report(
                    diagnostics,
                    format!("sizeof: unknown type '{}'", type_name),
                    ty.get_span(),
                );
                return Ok(Type::Unknown);
            }
        };

        let int_ty = Self::type_from_name("int");
        let ty_id = self.type_ctx.interner.int();
        self.hir_set_last(Some(HExpr::new(ty_id, HExprKind::IntLit(size as i64))));
        Ok(int_ty)
    }

    /// Byte size of a type name under Dream's ABI (matches `scalar_size` / struct tables).
    fn sizeof_bytes(&self, type_name: &str) -> Option<u32> {
        if let Some(info) = self.struct_table.get_struct(type_name) {
            if info.is_value {
                return Some(info.size as u32);
            }
            return Some(4);
        }
        if type_name.ends_with("[]") {
            return Some(4);
        }
        if type_name == "string"
            || type_name == "object"
            || type_name == "js"
            || type_name == "void"
        {
            return Some(4);
        }
        if self.enum_table.contains_key(type_name) {
            return Some(4);
        }
        if self.interface_methods.contains_key(type_name) {
            return Some(4);
        }
        if type_name.starts_with("fun(") {
            return Some(4);
        }
        if type_name.starts_with("Future<") {
            return Some(4);
        }
        if type_name.starts_with("Result<") {
            return Some(4);
        }
        if type_name.starts_with("Option<") {
            return Some(4);
        }

        match type_name {
            "int" | "uint" | "float" | "char" | "byte" | "bool" | "long" | "ulong" | "double" => {
                Some(value_size_align(type_name).0 as u32)
            }
            _ => None,
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
        let name = parts.last().unwrap().text.clone();
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
