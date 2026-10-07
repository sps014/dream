//! Binary-operator typing: null-coalescing, string concatenation, user-defined `equals` dispatch,
//! comparisons, and arithmetic.

use super::*;
use crate::errors::SemanticError;
use crate::function_table::FunctionIdentity;
use crate::symbol_table::SymbolTable;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{ExpressionNode, FunctionNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;
use dream_types::{DefKind, TyKind, TypeId};
use std::cell::RefCell;
use std::rc::Rc;

/// True for the binary bitwise operator tokens (`&`/`|`/`^`/`<<`/`>>`) — the ones restricted to
/// integer operands. `&&`/`||` are logical (bool-only, checked elsewhere), not bitwise.
fn is_bitwise_op(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::BitWiseAmpersandToken
            | TokenKind::BitWisePipeToken
            | TokenKind::BitWiseXorToken
            | TokenKind::ShiftLeftToken
            | TokenKind::ShiftRightToken
    )
}

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_binary_expression(
        &mut self,
        left: &'a ExpressionNode<'a>,
        opr: &SyntaxToken,
        right: &'a ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        // Don't leak an outer expected type (e.g. `double` from an assignment) into operands —
        // that would retarget `48` in `(int)c - 48` when the sum is later cast/added to double.
        let saved_expected = self.current_expected_type.take();
        let left_value =
            self.analyze_expression(left, parent_function, symbol_table, diagnostics)?;
        let left_hir = self.hir_take();

        // `x is T t && t.ok()`: every `is`-binding guaranteed by `left` (a bare `is`, or reachable
        // through a top-level `&&` chain within `left` itself) is visible while analyzing `right`,
        // since short-circuiting means `right` only ever runs once `left` is true. The alias is
        // popped again immediately after, so it never leaks past this one conjunct.
        let alias_mark = self.is_binding_aliases.len();
        if opr.kind == TokenKind::AmpersandAmpersandToken {
            let mut bindings = Vec::new();
            Self::collect_is_bindings(left, &mut bindings);
            for (name, ty, operand) in bindings {
                self.is_binding_aliases
                    .push((name.text.clone(), ty.clone(), operand));
            }
        }
        // An unsuffixed integer literal on the right takes the left operand's integer type, so the
        // parser's `x++` / `x += 1` desugaring (`x = x + 1`) type-checks for `byte`/`uint`/`ulong`.
        if matches!(right, ExpressionNode::Literal(Type::Integer(_))) && left_value.is_integer() {
            self.current_expected_type = Some(left_value.clone());
        }
        let right_value =
            self.analyze_expression(right, parent_function, symbol_table, diagnostics)?;
        let right_hir = self.hir_take();
        self.current_expected_type = None;
        self.is_binding_aliases.truncate(alias_mark);
        self.current_expected_type = saved_expected;

        if left_value.is_unknown() || right_value.is_unknown() {
            return Ok(Type::Unknown);
        }

        // `a ?? b`: pure sugar for `a.unwrap_or(b)` on an `Option<T>` left operand — the same
        // method the stdlib already exposes, just spelled as an operator for the common inline
        // "unwrap with a default" case. `a` must be `Option<T>`; `b` must be assignable to `T`.
        if opr.kind == TokenKind::QuestionQuestionToken {
            let Some((base, args)) = Self::resolve_struct_parts(&left_value) else {
                diagnostics.report_error(
                    format!(
                        "'??' requires an Option<T> operand, got {}",
                        self.ty_display(&left_value)
                    ),
                    Some(opr.position),
                );
                return Ok(Type::Unknown);
            };
            let option_def = self.type_ctx.resolve(DefKind::Union, &base);
            if option_def.is_none_or(|def| self.type_ctx.defs.name(def) != "Option")
                || args.len() != 1
            {
                diagnostics.report_error(
                    format!(
                        "'??' requires an Option<T> operand, got {}",
                        self.ty_display(&left_value)
                    ),
                    Some(opr.position),
                );
                return Ok(Type::Unknown);
            }
            self.ensure_union_instantiated(&base, &args, &opr.position, diagnostics);
            let inner = args[0].clone();
            self.compare_data_type(&inner, &right_value, &opr.position, diagnostics)?;
            if diagnostics.has_errors() {
                self.hir_none();
                return Ok(Type::Unknown);
            }
            let recv = left_hir
                .as_ref()
                .map(|hir| hir.ty)
                .unwrap_or_else(|| self.type_ctx.lower(&left_value));
            let inner_id = self.type_ctx.lower(&inner);
            let Some(method) = self.binary_instance_method(recv, "unwrap_or", inner_id) else {
                crate::internal_error!("validated Option type has no unwrap_or method");
            };
            self.hir_set_method_call(left_hir, &method, vec![right_hir], &inner);
            return Ok(inner);
        }

        // String concatenation: `string + T` (or `T + string`) yields a string, auto-converting
        // the non-string operand through its `to_string` (the object protocol, or a C-style enum's
        // variant-name rendering) in codegen. This means `"count = " + n` works for any `n` with no
        // explicit `.to_string()`.
        if opr.kind == TokenKind::PlusToken {
            let left_is_string = left_value.is_string();
            let right_is_string = right_value.is_string();
            if left_is_string || right_is_string {
                self.hir_set_concat(left_hir, &left_value, right_hir, &right_value);
                return Ok(if left_is_string {
                    left_value
                } else {
                    right_value
                });
            }
        }

        let operator_kind = if opr.kind == TokenKind::NotEqualToken {
            TokenKind::EqualEqualToken
        } else {
            opr.kind
        };
        match self.operator_binary_fn(&left_value, &right_value, operator_kind) {
            Ok(Some(method)) => {
                if let Some(target) = method.param_type {
                    let arg = self.type_ctx.lower(&right_value);
                    if !self.value_type_assignable(target, arg, diagnostics) {
                        diagnostics.report_error(
                            format!(
                                "Operator '{}' expects {}, got {}",
                                opr.text,
                                self.type_id_display(target),
                                self.type_id_display(arg)
                            ),
                            Some(opr.position),
                        );
                        return Ok(Type::Unknown);
                    }
                }
                let right_hir = right_hir.map(|value| match method.param_type {
                    Some(target) => self.coerce_to(value, target),
                    None => value,
                });
                self.hir_set_method_call(
                    left_hir,
                    &method.identity,
                    vec![right_hir],
                    &method.return_type,
                );
                if opr.kind == TokenKind::NotEqualToken {
                    self.hir_negate_last();
                }
                return Ok(method.return_type);
            }
            Ok(None) => {}
            Err(message) => {
                diagnostics.report_error(message, Some(opr.position));
                return Ok(Type::Unknown);
            }
        }

        // User-defined ordering: `@operator`-free structs implementing `Comparable<Self>` get
        // `<`/`<=`/`>`/`>=` for free, lowered to `a.compare(b) <op> 0`.
        if matches!(
            opr.kind,
            TokenKind::GreaterThanToken
                | TokenKind::GreaterThanEqualToken
                | TokenKind::SmallerThanToken
                | TokenKind::SmallerThanEqualToken
        ) && let Some(compare_fn) = self.comparable_compare_fn(&left_value)
        {
            self.compare_data_type(&left_value, &right_value, &opr.position, diagnostics)?;
            let bool_ty = Type::Boolean(opr.clone());
            let int_ty = Type::Integer(opr.clone());
            self.hir_set_method_call(left_hir, &compare_fn, vec![right_hir], &int_ty);
            self.hir_compare_last_to_zero(opr.kind);
            return Ok(bool_ty);
        }

        let left_ty = self.type_ctx.lower(&left_value);
        let right_ty = self.type_ctx.lower(&right_value);
        if !self.value_type_assignable(left_ty, right_ty, diagnostics) {
            diagnostics.report_error(
                format!(
                    "cannot convert from {} to {}",
                    self.ty_display(&right_value),
                    self.ty_display(&left_value)
                ),
                Some(opr.position),
            );
            return Ok(Type::Unknown);
        }

        // Bitwise ops (`&`/`|`/`^`/`<<`/`>>`) are only meaningful on integer operands
        // (`int`/`long`/`uint`/`ulong`/`byte`); `float`/`double` have no well-defined bitwise
        // lowering. Caught here rather than left to the backend, which would otherwise emit an
        // invalid WASM instruction (e.g. a nonexistent `f64.and`) instead of a clean diagnostic.
        // C-style enums are `i32` at runtime, so `&`/`|`/`^` combine them as bitflags (result
        // stays the enum type). Shifts stay integer-only — an enum is not a shift count.
        if is_bitwise_op(opr.kind) && !left_value.is_unknown() && !left_value.is_integer() {
            let enum_bitflags = matches!(
                opr.kind,
                TokenKind::BitWiseAmpersandToken
                    | TokenKind::BitWisePipeToken
                    | TokenKind::BitWiseXorToken
            ) && self.is_c_style_enum(&left_value);
            if !enum_bitflags {
                diagnostics.report_error(
                    format!(
                        "'{}' requires an integer operand (int/long/uint/ulong/byte), got {}",
                        opr.text,
                        self.ty_display(&left_value)
                    ),
                    Some(opr.position),
                );
                return Ok(Type::Unknown);
            }
        }

        match (&left_value, &opr.kind) {
            (Type::String(_), TokenKind::PlusToken) => {}
            // Reference (identity) equality is allowed on strings and objects.
            (Type::String(_), TokenKind::EqualEqualToken)
            | (Type::String(_), TokenKind::NotEqualToken) => {}
            (Type::String(_), _) => {
                diagnostics.report_error(
                    format!("Cannot perform operation {} on string", opr.text),
                    Some(opr.position),
                );
                return Ok(Type::Unknown);
            }
            (_, _) => {}
        };

        // User-defined value equality: for `==`/`!=` where the operand's static type is a user type
        // that implements `Equatable<Self>`, dispatch to its `equals` method (a static call),
        // negating the result for `!=`. Primitives, strings, and null comparisons keep the built-in
        // behavior handled above/below.
        if matches!(
            opr.kind,
            TokenKind::EqualEqualToken | TokenKind::NotEqualToken
        ) && let Some(equals_fn) = self.equatable_equals_fn(&left_value)
        {
            let bool_ty = Type::Boolean(opr.clone());
            self.hir_set_method_call(left_hir, &equals_fn, vec![right_hir], &bool_ty);
            if opr.kind == TokenKind::NotEqualToken {
                self.hir_negate_last();
            }
            return Ok(bool_ty);
        }

        let is_bool_result = matches!(
            opr.kind,
            TokenKind::EqualEqualToken
                | TokenKind::NotEqualToken
                | TokenKind::GreaterThanToken
                | TokenKind::GreaterThanEqualToken
                | TokenKind::SmallerThanToken
                | TokenKind::SmallerThanEqualToken
                | TokenKind::AmpersandAmpersandToken
                | TokenKind::PipePipeToken
        );
        let result_type = if is_bool_result {
            Type::Boolean(opr.clone())
        } else {
            left_value.clone()
        };
        self.hir_set_binary(left_hir, opr, right_hir, &result_type);
        Ok(result_type)
    }

    fn binary_instance_method(
        &self,
        receiver: TypeId,
        member: &str,
        argument: TypeId,
    ) -> Option<FunctionIdentity> {
        let candidates = self.function_table.method_candidates(receiver, member);
        let mut matches = candidates.into_iter().filter(|identity| {
            self.function_table
                .functions
                .get(identity)
                .is_some_and(|info| {
                    !info.is_static && info.parameters.as_slice() == [receiver, argument]
                })
        });
        let identity = matches.next()?;
        matches.next().is_none().then_some(identity)
    }

    fn equatable_equals_fn(&mut self, left: &Type) -> Option<FunctionIdentity> {
        self.comparison_method(left, "Equatable", "equals")
    }

    fn comparable_compare_fn(&mut self, left: &Type) -> Option<FunctionIdentity> {
        self.comparison_method(left, "Comparable", "compare")
    }

    fn comparison_method(
        &mut self,
        left: &Type,
        interface: &str,
        member: &str,
    ) -> Option<FunctionIdentity> {
        let receiver = self.type_ctx.lower(left);
        if !matches!(
            self.type_ctx.interner.kind(receiver),
            TyKind::Struct(..) | TyKind::Union(..)
        ) {
            return None;
        }
        let def = self.type_ctx.resolve(DefKind::Interface, interface)?;
        let iface = self.type_ctx.instantiate(def, vec![receiver]);
        if !self.class_implements(receiver, iface) {
            return None;
        }
        self.binary_instance_method(receiver, member, receiver)
    }
}
