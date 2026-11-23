//! Analysis of statements and control flow, split by concern:
//! - [`loops`]: `break`/`continue` placement checks and `while`/`do-while`/`for`/`for-each` loops.
//! - [`case_switch`]: the C-style `switch`/`case` statement (int/string/bool/enum subjects).
//! - [`bindings`]: `let` declarations, simple assignments, and `return`.
//! - [`assignments`]: indexed (`arr[i] = v`) and member (`obj.m = v`) writes and their desugaring.
//! - [`lvalue_operands`]: single evaluation of compound-assignment and `++`/`--` targets.
//! - [`conditionals`]: `if`/`else if`/`else`, including the compile-time `is` fold.
//!
//! The reserved-name check, the bool-condition diagnostic, and the `is`-with-binding flow-typing
//! helpers live here because they are shared across several of those submodules.

use super::*;
use crate::errors::SemanticError;
use crate::symbol_table::SymbolTable;
use dream_abi::intrinsics;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{ExpressionNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;
use std::cell::RefCell;
use std::rc::Rc;

mod assignments;
mod bindings;
mod case_switch;
mod conditionals;
mod defer_stmt;
mod indexer_writeback;
mod lock;
mod loops;
mod lvalue_operands;
mod overflow_block;

impl<'a> Analyzer<'a> {
    /// Reports a diagnostic when a control-flow condition is not `bool`. Already-`Unknown` conditions
    /// are skipped (their underlying error was reported where the type was produced). `context` names
    /// the construct for the message, e.g. "if" / "while" / "do/while" / "for".
    fn check_bool_condition(
        &mut self,
        context: &str,
        cond_type: &Type,
        position: Option<TextSpan>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if !cond_type.is_unknown() && !cond_type.is_bool() {
            diagnostics.report_error(
                format!(
                    "{} condition must be bool, got {}",
                    context,
                    self.ty_display(cond_type)
                ),
                position,
            );
        }
    }

    /// Reports a clear diagnostic when a reserved word (a builtin name or primitive type name) is
    /// used where a user-chosen identifier is expected (`role` is e.g. "variable"/"function").
    pub(in crate::analyzer) fn check_reserved_name(
        &self,
        token: &SyntaxToken,
        role: &str,
        diagnostics: &mut DiagnosticBag,
    ) {
        const RESERVED_TYPE_AND_LITERAL_NAMES: &[&str] = &[
            "int", "float", "float16", "double", "string", "bool", "char", "object", "void",
            "long", "uint", "ulong", "byte", "sbyte", "short", "ushort", "true", "false", "null",
        ];
        // The builtin callables are reserved too; sourced from the intrinsic registry so this list
        // never drifts from the set of names the compiler special-cases.
        let is_reserved = RESERVED_TYPE_AND_LITERAL_NAMES.contains(&token.text.as_str())
            || intrinsics::is_object_builtin(&token.text);
        if is_reserved {
            diagnostics.report_error(
                format!(
                    "'{}' is a reserved word and cannot be used as a {} name",
                    token.text, role
                ),
                Some(token.position),
            );
        }
    }

    /// A fresh child symbol scope of `parent`, used for a single `if`/`else if` branch (or a loop
    /// body) so an `is`-with-binding local lives only inside that branch and never leaks to
    /// `else`/the enclosing scope. Outer names stay visible through the parent link.
    fn branch_scope(&self, parent: &Rc<RefCell<SymbolTable>>) -> Rc<RefCell<SymbolTable>> {
        let scope = Rc::new(RefCell::new(SymbolTable::new(Some(Rc::clone(parent)))));
        (*parent).borrow_mut().add_child(scope.clone());
        scope
    }

    /// Collects the `is`-with-binding conditions that are guaranteed true whenever `cond` is true:
    /// a bare `x is T name`, and every such test reachable through a top-level `&&` chain (descending
    /// through parentheses). `||`, negation, and other operators are *not* descended into, because a
    /// binding under them is not guaranteed to hold when the whole condition is true. The collected
    /// bindings are declared into the taken branch (or loop body) by [`Self::declare_is_bindings`].
    pub(in crate::analyzer) fn collect_is_bindings<'e>(
        cond: &'e ExpressionNode<'a>,
        out: &mut Vec<(&'e SyntaxToken, &'e Type)>,
    ) {
        match cond {
            ExpressionNode::IsExpression(_, right_type, Some(name)) => {
                out.push((name, right_type));
            }
            ExpressionNode::Parenthesized(_, inner) => Self::collect_is_bindings(inner, out),
            ExpressionNode::Binary(left, op, right)
                if op.kind == TokenKind::AmpersandAmpersandToken =>
            {
                Self::collect_is_bindings(left, out);
                Self::collect_is_bindings(right, out);
            }
            _ => {}
        }
    }

    fn declare_is_bindings(
        &mut self,
        bindings: &[(&SyntaxToken, &Type)],
        branch_scope: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for &(name, target) in bindings {
            self.check_reserved_name(name, "variable", diagnostics);
            let target = Self::monomorphize_type(target, &self.current_generic_bindings);
            let value = self.hir_read_is_binding(name.position);
            self.hir_declare_local(&name.text, &target, value);
            if let Err(error) = branch_scope
                .borrow_mut()
                .add_symbol(name.text.clone(), target)
            {
                diagnostics.report_error(error.to_string(), Some(name.position));
            }
        }
    }
}

mod weak_locals;
