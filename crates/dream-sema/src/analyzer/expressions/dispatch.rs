//! The `analyze_expression` dispatch match and the class-indexer read desugar it delegates to.

use super::*;
use crate::errors::SemanticError;
use crate::symbol_table::SymbolTable;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{ExpressionNode, FunctionNode, LambdaBody, LambdaNode, Type};
use dream_syntax::token::token_kind::TokenKind;
use std::cell::RefCell;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn analyze_expression(
        &mut self,
        expression: &ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        match expression {
            ExpressionNode::Literal(number) => {
                let mut ty =
                    Self::retarget_numeric_literal(number, self.current_expected_type.as_ref());

                if let Type::Integer(t) | Type::Long(t) | Type::UInt(t) | Type::ULong(t) | Type::Byte(t) = &ty {
                    if matches!(ty, Type::ULong(_)) {
                        if dream_syntax::number::parse_u64_literal(&t.text).is_none() {
                            diagnostics.report_error(
                                format!("integer literal '{}' is out of range or malformed", t.text),
                                number.get_span(),
                            );
                        }
                    } else if let Some(val) = dream_syntax::number::parse_int_literal(&t.text) {
                        let err = match &ty {
                            Type::Integer(_) => val < i32::MIN as i64 || val > i32::MAX as i64,
                            Type::Byte(_) => !(0i64..=255).contains(&val),
                            Type::UInt(_) => val < 0 || val > u32::MAX as i64,
                            Type::Long(_) => false,
                            _ => false,
                        };
                        if err && matches!(ty, Type::Integer(_)) {
                            ty = Type::Long(t.clone());
                        } else if err {
                            diagnostics.report_error(format!("literal {} does not fit in {}", t.text, ty.get_type()), number.get_span());
                        }
                    } else {
                        diagnostics.report_error(format!("integer literal '{}' is out of range or malformed", t.text), number.get_span());
                    }
                } else if let Type::Float(t) | Type::Double(t) = &ty {
                    if dream_syntax::number::parse_float_literal(&t.text).is_none() {
                        diagnostics.report_error(format!("float literal '{}' is out of range or malformed", t.text), number.get_span());
                    }
                }

                self.hir_set_literal(&ty);
                Ok(ty)
            }
            ExpressionNode::ArrayLiteral(open, elements) => {
                // `[e1, e2, ...]` lowers to `List<T>.from_array([e1, e2, ...])` (one bulk call, no
                // per-element codegen) whenever the surrounding context expects a `List<T>`; the
                // `T[]` array form below is otherwise completely unaffected.
                if let Some(elem_ty) = self
                    .current_expected_type
                    .as_ref()
                    .and_then(|t| Self::collection_generic_arg(t, "List"))
                {
                    let ctx = super::super::AnalyzerContext {
                        parent_function,
                        symbol_table,
                    };
                    return self.lower_collection_literal_call(
                        "List",
                        vec![elem_ty],
                        "from_array",
                        vec![ExpressionNode::ArrayLiteral(open.clone(), elements.clone())],
                        &ctx,
                        diagnostics,
                    );
                }

                // The element type expected for this literal, taken from the surrounding array-typed
                // context (`let xs: int[] = ...`, `return ...`, an argument slot, a field, etc.). It
                // is threaded down into each element so nested empty literals (`int[][] = [[]]`) and
                // empty elements infer their element type instead of falling through as untyped.
                let expected_elem = match &self.current_expected_type {
                    Some(Type::Array(elem)) => Some((**elem).clone()),
                    _ => None,
                };

                if elements.is_empty() {
                    // With an array-typed context the empty literal takes that element type; without
                    // one it is genuinely ambiguous (nothing to infer from), so reject it clearly.
                    if let Some(elem) = expected_elem {
                        self.hir_set_empty_array(&elem);
                        return Ok(Type::Array(Box::new(elem)));
                    }
                    self.hir_none();
                    self.hir_fail();
                    diagnostics.report_error(
                        "cannot infer the element type of an empty array literal; add an array type annotation, e.g. `let xs: int[] = [];`".to_string(),
                        expression.position(),
                    );
                    return Ok(Type::Array(Box::new(Type::Void)));
                }

                let saved_expected = self.current_expected_type.take();
                self.current_expected_type = expected_elem;
                let first_type = self.analyze_expression(
                    &elements[0],
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                let mut elem_hirs = vec![self.hir_take()];

                for elem in elements.iter().skip(1) {
                    let element_type =
                        self.analyze_expression(elem, parent_function, symbol_table, diagnostics)?;
                    elem_hirs.push(self.hir_take());
                    let span = elem.position().unwrap_or(open.position);
                    self.compare_data_type(&first_type, &element_type, &span, diagnostics)?;
                }
                self.current_expected_type = saved_expected;

                let array_type = Type::Array(Box::new(first_type));
                self.hir_set_array_lit(elem_hirs, &array_type);
                Ok(array_type)
            }
            ExpressionNode::ArrayRepeat(open, value, len) => {
                // `[v; n]` in a `List<T>` context composes with the collection-literal desugar:
                // replay as `List<T>.from_array([v; n])`, re-entering this arm for the inner
                // repeat once the callee signature publishes its `T[]` parameter expectation.
                if let Some(elem_ty) = self
                    .current_expected_type
                    .as_ref()
                    .and_then(|t| Self::collection_generic_arg(t, "List"))
                {
                    let ctx = super::super::AnalyzerContext {
                        parent_function,
                        symbol_table,
                    };
                    return self.lower_collection_literal_call(
                        "List",
                        vec![elem_ty],
                        "from_array",
                        vec![ExpressionNode::ArrayRepeat(
                            open.clone(),
                            Box::new((**value).clone()),
                            Box::new((**len).clone()),
                        )],
                        &ctx,
                        diagnostics,
                    );
                }

                let expected_elem = match &self.current_expected_type {
                    Some(Type::Array(elem)) => Some((**elem).clone()),
                    _ => None,
                };

                // Zero-like scalar init (`[0; n]`, `[0.0; n]`, `[false; n]`): every runtime
                // allocation is zero-filled anyway, so lower straight to the `Buffer.alloc`
                // intrinsic and skip the per-slot fill entirely. Only for scalar element types —
                // for reference elements (`string[]` from `[0; n]`) zero-fill would produce null
                // refs, and the general path below reports the mismatch instead.
                if Self::is_zero_like_literal(value)
                    && expected_elem
                        .as_ref()
                        .is_none_or(Self::is_scalar_value_type)
                {
                    let elem_ty = match expected_elem {
                        Some(t) => t,
                        None => {
                            let saved = self.current_expected_type.take();
                            let ty = self.analyze_expression(
                                value,
                                parent_function,
                                symbol_table,
                                diagnostics,
                            )?;
                            self.current_expected_type = saved;
                            let _ = self.hir_take();
                            ty
                        }
                    };
                    let len_ty =
                        self.analyze_expression(len, parent_function, symbol_table, diagnostics)?;
                    let len_hir = self.hir_take();
                    let int_ty = Type::Integer(synthetic_token(TokenKind::DataTypeToken, "int"));
                    if !len_ty.is_unknown() && len_ty != int_ty {
                        let span = len.position().unwrap_or(open.position);
                        self.compare_data_type(&int_ty, &len_ty, &span, diagnostics)?;
                    }
                    self.hir_set_array_new(&elem_ty, len_hir);
                    return Ok(Type::Array(Box::new(elem_ty)));
                }

                // General case, replayed through ordinary static dispatch on the bootstrap
                // `Array` class (single analysis of both operands — no duplicated diagnostics).
                // When the value builds an array at its top level (`[[0; 3]; n]`) every slot must
                // receive a *fresh* row, so the value is re-evaluated per index via
                // `Array.repeat_with<T>(n, () => v)`; otherwise it is evaluated exactly once and
                // shared (`Array.repeat<T>(n, v)`, ARC-retain per slot).
                let repeat_with = Self::is_array_construction(value);
                let method = if repeat_with { "repeat_with" } else { "repeat" };
                let args = if repeat_with {
                    let body = self.arena.alloc((**value).clone());
                    let lambda = ExpressionNode::Lambda(self.arena.alloc(LambdaNode {
                        open_paren_position: open.position,
                        async_keyword: None,
                        is_async: false,
                        generic_parameters: None,
                        generic_constraints: Vec::new(),
                        parameters: Vec::new(),
                        body: LambdaBody::Expr(body),
                    }));
                    vec![(**len).clone(), lambda]
                } else {
                    vec![(**len).clone(), (**value).clone()]
                };
                let receiver = self.arena.alloc(ExpressionNode::Identifier(synthetic_token(
                    TokenKind::IdentifierToken,
                    "Array",
                )));
                let synthetic = ExpressionNode::MethodCall(
                    receiver,
                    synthetic_token(TokenKind::IdentifierToken, method),
                    None,
                    args,
                );
                self.analyze_expression(&synthetic, parent_function, symbol_table, diagnostics)
            }
            ExpressionNode::TupleLiteral(_, elements) => {
                if elements.len() < 2 {
                    self.hir_fail();
                    diagnostics.report_error(
                        "Tuple literals require at least two elements".to_string(),
                        expression.position(),
                    );
                    return Ok(Type::Unknown);
                }
                let expected_elems: Option<Vec<Type>> = match &self.current_expected_type {
                    Some(Type::Tuple(elems)) if elems.len() == elements.len() => {
                        Some(elems.clone())
                    }
                    _ => None,
                };
                let mut elem_tys = Vec::with_capacity(elements.len());
                let mut elem_hirs = Vec::with_capacity(elements.len());
                for (i, elem) in elements.iter().enumerate() {
                    let saved = self.current_expected_type.take();
                    self.current_expected_type =
                        expected_elems.as_ref().and_then(|es| es.get(i).cloned());
                    let ty = self
                        .analyze_expression(elem, parent_function, symbol_table, diagnostics)
                        .unwrap_or(Type::Unknown);
                    elem_hirs.push(self.hir_take());
                    self.current_expected_type = saved;
                    if let Some(es) = expected_elems.as_ref() {
                        let span = elem.position().unwrap_or_else(empty_span);
                        self.compare_data_type(&es[i], &ty, &span, diagnostics)?;
                        elem_tys.push(es[i].clone());
                    } else {
                        elem_tys.push(ty);
                    }
                }
                let tuple_ty = Type::Tuple(elem_tys);
                self.hir_set_tuple_lit(elem_hirs, &tuple_ty);
                Ok(tuple_ty)
            }
            ExpressionNode::SetLiteral(open, elements) => {
                // A Set literal always requires an expected `Set<T>` target type (unlike `[...]`,
                // there is no bare-element fallback type to infer). An empty `{}` is ambiguous with
                // an empty map, so it is reinterpreted as one here when the context calls for it.
                match self.current_expected_type.clone() {
                    Some(t)
                        if elements.is_empty()
                            && Self::collection_generic_arg2(&t, "Map").is_some() =>
                    {
                        self.analyze_expression(
                            &ExpressionNode::MapLiteral(open.clone(), vec![]),
                            parent_function,
                            symbol_table,
                            diagnostics,
                        )
                    }
                    Some(t) => {
                        let Some(elem_ty) = Self::collection_generic_arg(&t, "Set") else {
                            self.hir_none();
                            self.hir_fail();
                            diagnostics.report_error(
                                format!(
                                    "cannot use a Set literal where a '{}' is expected",
                                    self.ty_display(&t)
                                ),
                                expression.position(),
                            );
                            return Ok(Type::Unknown);
                        };
                        let ctx = super::super::AnalyzerContext {
                            parent_function,
                            symbol_table,
                        };
                        let bracket = synthetic_token(TokenKind::OpenBracketToken, "[");
                        self.lower_collection_literal_call(
                            "Set",
                            vec![elem_ty],
                            "from_array",
                            vec![ExpressionNode::ArrayLiteral(bracket, elements.clone())],
                            &ctx,
                            diagnostics,
                        )
                    }
                    None => {
                        self.hir_none();
                        self.hir_fail();
                        diagnostics.report_error(
                            "a Set literal requires a target type, e.g. `let s: Set<int> = {1, 2};`".to_string(),
                            expression.position(),
                        );
                        Ok(Type::Unknown)
                    }
                }
            }
            ExpressionNode::MapLiteral(_, entries) => {
                // A Map literal always requires an expected `Map<K, V>` target type, for the same
                // reason as `SetLiteral` above.
                let Some((key_ty, val_ty)) = self
                    .current_expected_type
                    .clone()
                    .and_then(|t| Self::collection_generic_arg2(&t, "Map"))
                else {
                    self.hir_none();
                    self.hir_fail();
                    diagnostics.report_error(
                        "a Map literal requires a target type, e.g. `let m: Map<string, int> = {\"a\": 1};`".to_string(),
                        expression.position(),
                    );
                    return Ok(Type::Unknown);
                };
                let (keys, values): (Vec<_>, Vec<_>) = entries.iter().cloned().unzip();
                let ctx = super::super::AnalyzerContext {
                    parent_function,
                    symbol_table,
                };
                let bracket = synthetic_token(TokenKind::OpenBracketToken, "[");
                self.lower_collection_literal_call(
                    "Map",
                    vec![key_ty, val_ty],
                    "from_arrays",
                    vec![
                        ExpressionNode::ArrayLiteral(bracket.clone(), keys),
                        ExpressionNode::ArrayLiteral(bracket, values),
                    ],
                    &ctx,
                    diagnostics,
                )
            }
            ExpressionNode::IndexAccess(array_expr, index_expr) => {
                // Don't leak an outer expected type (e.g. `double` from `unwrap(): T`) into the
                // index: `Buffer.alloc<T>(1)[0]` would retarget the literal `0` to `double`.
                let saved_expected = self.current_expected_type.take();
                let array_type = self.analyze_expression(
                    array_expr,
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                let array_hir = self.hir_take();

                // A `js`-typed receiver indexes dynamically (`obj[key]`), with a string or numeric
                // key. Must precede the class/string indexer desugar, which would look for a `get`.
                if self.is_js_type(&array_type) {
                    let key_type = self.analyze_expression(
                        index_expr,
                        parent_function,
                        symbol_table,
                        diagnostics,
                    )?;
                    let key_hir = self.hir_take();
                    let _ = key_type;
                    self.desugar_js_index_get(
                        array_hir,
                        key_hir,
                        index_expr.position(),
                        diagnostics,
                    );
                    self.current_expected_type = saved_expected;
                    return Ok(Self::js_type());
                }

                // Inside `@compute`, `GpuBuffer<T>` indexes like `T[]` (storage buffer elements).
                // Do not use host `@get_indexer` — CPU GpuBuffer has no indexer by design.
                let gpu_elem = if self.current_function_is_gpu {
                    crate::analyzer::declarations::functions::gpu_buffer_elem_type(&array_type)
                        .cloned()
                } else {
                    None
                };

                // Class/string indexer: `obj[i]` on a struct or `string` receiver desugars to
                // `obj.get(i)` when an eligible `get` exists (`string` exposes one via `extend
                // string`, yielding a `char`). Arrays keep the built-in index path; `Unknown` is a
                // poison carried from an earlier error and must not cascade.
                if gpu_elem.is_none()
                    && !matches!(array_type, Type::Array(_) | Type::Unknown)
                    && (Self::resolve_struct_parts(&array_type).is_some()
                        || matches!(array_type, Type::String(_)))
                {
                    // The synthesized call re-evaluates the receiver, so drop the base HIR taken above.
                    let _ = array_hir;
                    let result = self.analyze_index_get(
                        array_expr,
                        index_expr,
                        &array_type,
                        parent_function,
                        symbol_table,
                        diagnostics,
                    );
                    self.current_expected_type = saved_expected;
                    return result;
                }

                let inner_type = match (gpu_elem, array_type) {
                    (Some(elem), _) => elem,
                    (_, Type::Array(inner)) => *inner,
                    // Don't cascade if the base was already poisoned by an earlier error.
                    (_, Type::Unknown) => Type::Unknown,
                    (_, other) => {
                        diagnostics.report_error(
                            format!("Cannot index into non-array type {}", self.ty_display(&other)),
                            array_expr.position(),
                        );
                        Type::Unknown
                    }
                };

                let index_type = self.analyze_expression(
                    index_expr,
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                let index_hir = self.hir_take();
                if !index_type.is_unknown() && !index_type.is_int() {
                    diagnostics.report_error(
                        format!(
                            "Array index must be of type int, got {}",
                            self.ty_display(&index_type)
                        ),
                        index_expr.position(),
                    );
                }

                self.hir_set_index(array_hir, index_hir, &inner_type);
                self.current_expected_type = saved_expected;
                Ok(inner_type)
            }
            ExpressionNode::Unary(opr, right) => {
                let right_type =
                    self.analyze_expression(right, parent_function, symbol_table, diagnostics)?;
                let operand = self.hir_take();
                // User-defined unary operator overload: `@operator("-")`/`@operator("!")`/
                // `@operator("~")` on the operand's type, checked before the built-in
                // bool/numeric/integer rules below so a struct's overload always wins.
                if let Some(op_method) = self.operator_unary_fn(&right_type, opr.kind) {
                    let return_type = op_method.return_type;
                    self.hir_set_method_call(
                        operand,
                        &op_method.mangled_name,
                        vec![],
                        &return_type,
                    );
                    return Ok(return_type);
                }
                match opr.kind {
                    TokenKind::BangToken => {
                        if !right_type.is_unknown() && !right_type.is_bool() {
                            diagnostics.report_error(
                                format!("! operator requires bool, got {}", self.ty_display(&right_type)),
                                Some(opr.position),
                            );
                            self.hir_none();
                            return Ok(Type::Unknown);
                        }
                        let result = Type::Boolean(opr.clone());
                        self.hir_set_unary(opr, operand, &result);
                        Ok(result)
                    }
                    TokenKind::PlusToken | TokenKind::MinusToken => {
                        if opr.kind == TokenKind::MinusToken
                            && self.try_gpu_unary_neg(&right_type, operand.clone())
                        {
                            return Ok(right_type);
                        }
                        if Analyzer::is_gpu_vec(&right_type) && opr.kind == TokenKind::PlusToken {
                            self.hir_set_unary(opr, operand, &right_type);
                            return Ok(right_type);
                        }
                        if !right_type.is_unknown()
                            && !matches!(
                                right_type,
                                Type::Integer(_)
                                    | Type::Long(_)
                                    | Type::UInt(_)
                                    | Type::ULong(_)
                                    | Type::Byte(_)
                                    | Type::Float(_)
                                    | Type::Double(_)
                            )
                        {
                            diagnostics.report_error(
                                format!(
                                    "unary +/- requires a numeric type, got {}",
                                    self.ty_display(&right_type)
                                ),
                                Some(opr.position),
                            );
                            self.hir_none();
                            return Ok(Type::Unknown);
                        }
                        self.hir_set_unary(opr, operand, &right_type);
                        Ok(right_type)
                    }
                    TokenKind::TildeToken => {
                        if !right_type.is_unknown()
                            && !right_type.is_integer()
                            && !self.is_c_style_enum(&right_type)
                        {
                            diagnostics.report_error(
                                format!(
                                    "~ operator requires an integer operand (int/long/uint/ulong/byte), got {}",
                                    self.ty_display(&right_type)
                                ),
                                Some(opr.position),
                            );
                            self.hir_none();
                            return Ok(Type::Unknown);
                        }
                        self.hir_set_unary(opr, operand, &right_type);
                        Ok(right_type)
                    }
                    _ => {
                        diagnostics.report_error(
                            format!("unknown unary operator {}", opr.text),
                            Some(opr.position),
                        );
                        self.hir_none();
                        Ok(Type::Unknown)
                    }
                }
            }
            ExpressionNode::IncDec {
                prefix,
                is_inc,
                target,
                op,
            } => self.analyze_inc_dec(
                (*prefix, *is_inc),
                target,
                op,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            ExpressionNode::Binary(left, opr, right) => Ok(self.analyze_binary_expression(
                left,
                opr,
                right,
                parent_function,
                symbol_table,
                diagnostics,
            )?),
            ExpressionNode::Identifier(id) => {
                // An `is`-binding introduced earlier in the same top-level `&&` chain (see
                // `analyze_binary_expression`) shadows an ordinary local of the same name, exactly
                // like the real branch-body binding does.
                let alias = self
                    .is_binding_aliases
                    .iter()
                    .rev()
                    .find(|(name, _, _)| name == &id.text)
                    .map(|(_, ty, operand)| (ty.clone(), *operand));
                if let Some((target_ty, operand)) = alias {
                    return self.analyze_cast(
                        &target_ty,
                        operand,
                        parent_function,
                        symbol_table,
                        diagnostics,
                    );
                }
                Ok(self.analyze_identifier(id, symbol_table, diagnostics)?)
            }
            ExpressionNode::FunctionCall(name, generic_args, params) => {
                // `analyze_function_call` records the call's HIR itself (only for a resolvable,
                // non-generic, non-overloaded, non-async free function; otherwise it clears `last`).
                let t = self.analyze_function_call(
                    name,
                    generic_args,
                    params,
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                Ok(t)
            }
            ExpressionNode::Call(callee, generic_args, params) => self.analyze_expr_call(
                callee,
                generic_args,
                params,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            ExpressionNode::IsExpression(left, right_type, _binding) => {
                // `is` always evaluates to a bool. A concrete static operand folds to a compile-time
                // result; an `object` or interface-typed operand emits a runtime `$object_tag`
                // comparison. (The optional `_binding` is handled by the statement layer — `if`/
                // `while` conditions and top-level `&&` chains, see `statements.rs` — which flow-types
                // it into the guarded branch/body; the expression itself ignores the binding here.)
                let left_type =
                    self.analyze_expression(left, parent_function, symbol_table, diagnostics)?;
                self.check_type_not_static_class(right_type, diagnostics);
                let left_hir = self.hir_take();
                let left_name = left_type.get_type();
                if left_type.is_unknown() {
                    self.hir_none();
                } else if left_name == "object" || self.is_interface_name(&left_name) {
                    self.hir_set_is_type(left_hir, right_type);
                } else {
                    let left_id = self.type_ctx.lower(&left_type);
                    let right_id = self.type_ctx.lower(right_type);
                    self.hir_set_bool(left_id == right_id);
                }
                Ok(Type::Boolean(synthetic_token(
                    TokenKind::BooleanToken,
                    "true",
                )))
            }
            ExpressionNode::Parenthesized(_, expr) => {
                Ok(self.analyze_expression(expr, parent_function, symbol_table, diagnostics)?)
            }
            ExpressionNode::Try(inner) => Ok(self.analyze_try_expression(
                inner,
                parent_function,
                symbol_table,
                diagnostics,
            )?),
            ExpressionNode::Lambda(lambda) => {
                Ok(self.analyze_lambda(lambda, parent_function, symbol_table, diagnostics)?)
            }
            ExpressionNode::Ternary(condition, then_expr, else_expr) => {
                let cond_type =
                    self.analyze_expression(condition, parent_function, symbol_table, diagnostics)?;
                let cond_hir = self.hir_take();
                if !cond_type.is_bool() {
                    diagnostics.report_error(
                        format!(
                            "Ternary condition must be of type bool, got {}",
                            self.ty_display(&cond_type)
                        ),
                        condition.position(),
                    );
                }
                let then_type =
                    self.analyze_expression(then_expr, parent_function, symbol_table, diagnostics)?;
                let then_hir = self.hir_take();
                let else_type =
                    self.analyze_expression(else_expr, parent_function, symbol_table, diagnostics)?;
                let else_hir = self.hir_take();
                // Both branches must agree; reuse the standard compatibility check.
                self.compare_data_type(
                    &then_type,
                    &else_type,
                    &else_expr.position().unwrap_or_else(empty_span),
                    diagnostics,
                )?;
                self.hir_set_ternary(cond_hir, then_hir, else_hir, &then_type);
                Ok(then_type)
            }
            ExpressionNode::Switch(_, subject, arms) => {
                // `analyze_pattern_switch` desugars the value-position switch and records its result temp read.
                let t = self.analyze_pattern_switch(
                    subject,
                    arms,
                    parent_function,
                    symbol_table,
                    true,
                    diagnostics,
                )?;
                Ok(t)
            }
            ExpressionNode::MemberAccess(obj, member) => {
                // `analyze_member_access` records the HIR itself (struct-field read / enum value).
                let t = self.analyze_member_access(
                    obj,
                    member,
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                Ok(t)
            }
            ExpressionNode::Cast(_, target_type, expr) => {
                // `analyze_cast` records the cast's HIR itself.
                let t = self.analyze_cast(
                    target_type,
                    expr,
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                Ok(t)
            }
            ExpressionNode::SizeOf(_, ty) => self.analyze_sizeof(ty, diagnostics),
            ExpressionNode::NameOf(_, parts) => self.analyze_nameof(parts, diagnostics),
            ExpressionNode::MethodCall(obj, method, generic_args, params) => {
                let ctx = super::super::AnalyzerContext {
                    parent_function,
                    symbol_table,
                };
                let t =
                    self.analyze_method_call(obj, method, generic_args, params, &ctx, diagnostics)?;
                // `analyze_method_call` records the `MethodCall`/`Call` (or clears `last`) itself.
                Ok(t)
            }
            ExpressionNode::Await(_, inner) => {
                let fut =
                    self.analyze_expression(inner, parent_function, symbol_table, diagnostics)?;
                let inner_hir = self.hir_take();
                if fut.is_unknown() {
                    self.hir_none();
                    return Ok(Type::Unknown);
                }
                // Awaiting a dynamic `js` value treats it as a JS Promise: desugar to
                // `await js.await_promise(inner)`, whose async bridge yields `Future<js>` and resolves to
                // the awaited value as another `js`.
                if self.is_js_type(&fut) {
                    let fut_hir = self.desugar_js_await(inner_hir);
                    let opt = Self::option_js_type();
                    self.hir_set_await(fut_hir, &opt);
                    return Ok(opt);
                }
                match Self::future_inner_type(&fut) {
                    Some(t) => {
                        self.hir_set_await(inner_hir, &t);
                        Ok(t)
                    }
                    None => {
                        self.hir_none();
                        Err(report(
                            diagnostics,
                            format!("'await' expects a Future value, got {}", self.ty_display(&fut)),
                            inner.position(),
                        ))
                    }
                }
            }
            // A named argument (`name: value`) is only meaningful inside a call's argument list,
            // where `normalize_named_arguments` resolves and strips it before any argument reaches
            // general expression analysis. Reaching this arm means one appeared somewhere else
            // (e.g. `[a: 1]`) — report it as a diagnostic rather than silently analyzing `value`.
            ExpressionNode::NamedArg(name, _) => {
                self.hir_none();
                Err(report(
                    diagnostics,
                    format!("named argument '{}' is not allowed here", name.text),
                    Some(name.position),
                ))
            }
            // A `ref` argument (`f(ref x)`) is only meaningful inside a call's argument list,
            // where the call-analysis paths (`analyze_ref_argument`) resolve and strip it before
            // any argument reaches general expression analysis. Reaching this arm means one
            // appeared somewhere else (e.g. `let y = ref x;`) — report it, don't silently analyze
            // the inner place as if `ref` weren't there.
            ExpressionNode::RefArgument(_, inner) => {
                self.hir_none();
                Err(report(
                    diagnostics,
                    "'ref' is only allowed as a call argument".to_string(),
                    inner.position(),
                ))
            }
            // Syntax DSL blocks must be expanded by the generate pipeline before analysis.
            ExpressionNode::SyntaxBlock(block) => {
                self.hir_none();
                Err(report(
                    diagnostics,
                    format!(
                        "unexpanded syntax block '{}'; no generator ran for this introducer",
                        block.name.text
                    ),
                    Some(block.name.position),
                ))
            }
        }
    }

    /// Desugars a class indexer read `obj[index]` to a call of the type's `@get_indexer` method when
    /// registered (see [`declarations::protocol_hooks`]): an accessible instance, non-async
    /// method taking one argument and returning a (non-`void`) value.
    fn analyze_index_get(
        &mut self,
        array_expr: &'a ExpressionNode<'a>,
        index_expr: &'a ExpressionNode<'a>,
        obj_type: &Type,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        use crate::analyzer::declarations::protocol_hooks::ProtocolRole;
        let pretty_obj = self.ty_display(obj_type);
        let (hook, info) = match self.resolve_hook_or_diagnose(
            obj_type,
            ProtocolRole::Get,
            array_expr.position(),
            true,
            diagnostics,
            || {
                format!(
                    "type '{}' has no indexer (define '@get_indexer public fun ...(index): T' to allow obj[index])",
                    pretty_obj
                )
            },
        ) {
            Some(resolved) => resolved,
            None => return Ok(Type::Unknown),
        };
        if matches!(info.return_type, None | Some(Type::Void)) {
            self.hir_fail();
            self.hir_none();
            diagnostics.report_error(
                format!(
                    "type '{}' has no indexer: its '@get_indexer' method must return a value",
                    self.ty_display(obj_type)
                ),
                array_expr.position(),
            );
            return Ok(Type::Unknown);
        }
        let get_tok = synthetic_token(TokenKind::IdentifierToken, &hook.surface_name);
        let call =
            ExpressionNode::MethodCall(array_expr, get_tok, None, vec![(*index_expr).clone()]);
        self.analyze_expression(&call, parent_function, symbol_table, diagnostics)
    }

    /// True for scalar value types whose all-zero bit pattern is a valid value (`int`,
    /// `float`, `bool`, …). Excludes `string`/objects/classes, where zero means a null ref.
    fn is_scalar_value_type(ty: &Type) -> bool {
        matches!(
            ty,
            Type::Integer(_)
                | Type::Float(_)
                | Type::Double(_)
                | Type::Boolean(_)
                | Type::Byte(_)
                | Type::Char(_)
                | Type::Long(_)
                | Type::UInt(_)
                | Type::ULong(_)
        )
    }

    /// True for a literal whose every slot would already be produced by runtime zero-fill
    /// (`0`, `0.0`, `false`, `0` bytes), letting `[v; n]` skip the per-slot fill loop.
    fn is_zero_like_literal(expr: &ExpressionNode) -> bool {
        match expr {
            ExpressionNode::Literal(Type::Integer(t)) | ExpressionNode::Literal(Type::Byte(t)) => {
                t.text == "0"
            }
            ExpressionNode::Literal(Type::Float(t)) | ExpressionNode::Literal(Type::Double(t)) => {
                matches!(t.text.as_str(), "0" | "0.0")
            }
            ExpressionNode::Literal(Type::Boolean(t)) => t.text == "false",
            _ => false,
        }
    }

    /// True when the top level of `expr` constructs an array (`[...]` literal, `[v; n]` repeat,
    /// possibly parenthesized). Only then does `[v; n]` re-evaluate the value per slot so each
    /// row is distinct; deeper occurrences evaluate once like any other expression.
    fn is_array_construction(expr: &ExpressionNode) -> bool {
        match expr {
            ExpressionNode::Parenthesized(_, inner) => Self::is_array_construction(inner),
            ExpressionNode::ArrayLiteral(..) | ExpressionNode::ArrayRepeat(..) => true,
            _ => false,
        }
    }

    /// If `t` is `{name}<A>` (a one-generic-argument struct named `name`, e.g. `List<int>`),
    /// returns `A`. Used to recognize an expected `List<T>`/`Set<T>` target type for collection
    /// literal lowering.
    pub(in crate::analyzer) fn collection_generic_arg(t: &Type, name: &str) -> Option<Type> {
        match t {
            Type::Struct(tok, Some(args)) if tok.text == name && args.len() == 1 => {
                Some(args[0].clone())
            }
            _ => None,
        }
    }

    /// Like [`Self::collection_generic_arg`], but for a two-generic-argument struct (`Map<K, V>`).
    fn collection_generic_arg2(t: &Type, name: &str) -> Option<(Type, Type)> {
        match t {
            Type::Struct(tok, Some(args)) if tok.text == name && args.len() == 2 => {
                Some((args[0].clone(), args[1].clone()))
            }
            _ => None,
        }
    }

    /// Lowers a collection literal (`[...]` as `List<T>`, `{...}` as `Set<T>`/`Map<K, V>`) into a
    /// single synthetic static-factory call `{base}<{type_args}>.{method}(args)` and replays it
    /// through the ordinary `analyze_expression` path, reusing the existing generic-class
    /// static-dispatch machinery (the same one `Cache<int>.make(...)` uses) verbatim — no new HIR
    /// shape, no per-element codegen. `args` are typically one or two synthetic `ArrayLiteral`
    /// nodes wrapping the literal's original element/key/value sub-expressions; their element types
    /// are inferred from the (now type-argument-aware, see `analyze_static_call`) callee signature,
    /// so an empty literal like `let s: Set<int> = {};` still resolves correctly.
    fn lower_collection_literal_call(
        &mut self,
        base: &str,
        type_args: Vec<Type>,
        method: &str,
        args: Vec<ExpressionNode<'a>>,
        ctx: &super::super::AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let receiver = self.arena.alloc(ExpressionNode::Identifier(synthetic_token(
            TokenKind::IdentifierToken,
            base,
        )));
        let call = ExpressionNode::MethodCall(
            receiver,
            synthetic_token(TokenKind::IdentifierToken, method),
            Some(type_args),
            args,
        );
        self.analyze_expression(&call, ctx.parent_function, ctx.symbol_table, diagnostics)
    }

    /// When the surrounding context expects `double`, retarget unsuffixed float/int literals so
    /// `let x: double = 3.14` and `Math` double overloads don't require a `d` suffix. Explicit
    /// `f`/`d`/`L`/… suffixes are already classified by the parser; bare decimals arrive as
    /// `Float`, bare integers as `Integer`.
    fn retarget_numeric_literal(lit: &Type, expected: Option<&Type>) -> Type {
        match (expected, lit) {
            (Some(Type::Double(_)), Type::Float(t) | Type::Integer(t)) => Type::Double(t.clone()),
            (Some(Type::Float(_)), Type::Integer(t)) => Type::Float(t.clone()),
            (Some(Type::UInt(_)), Type::Integer(t) | Type::UInt(t)) => Type::UInt(t.clone()),
            (Some(Type::Long(_)), Type::Integer(t)) => Type::Long(t.clone()),
            (Some(Type::ULong(_)), Type::Integer(t)) => Type::ULong(t.clone()),
            (Some(Type::Byte(_)), Type::Integer(t)) => Type::Byte(t.clone()),
            _ => lit.clone(),
        }
    }
}
