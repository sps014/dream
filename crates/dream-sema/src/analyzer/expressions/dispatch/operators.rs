use super::*;

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_operators_expression(
        &mut self,
        expression: &ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        match expression {
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

                // Class/string indexer: `obj[i]` on a struct or `string` receiver desugars to
                // `obj.get(i)` when an eligible `get` exists (`string` exposes one via `extend
                // string`, yielding a `char`). Arrays keep the built-in index path; `Unknown` is a
                // poison carried from an earlier error and must not cascade.
                if !matches!(array_type, Type::Array(_) | Type::Unknown)
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

                let inner_type = match array_type {
                    Type::Array(inner) => *inner,
                    // Don't cascade if the base was already poisoned by an earlier error.
                    Type::Unknown => Type::Unknown,
                    other => {
                        diagnostics.report_error(
                            format!(
                                "Cannot index into non-array type {}",
                                self.ty_display(&other)
                            ),
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
                    self.hir_set_method_call(operand, &op_method.identity, vec![], &return_type);
                    return Ok(return_type);
                }
                match opr.kind {
                    TokenKind::BangToken => {
                        if !right_type.is_unknown() && !right_type.is_bool() {
                            diagnostics.report_error(
                                format!(
                                    "! operator requires bool, got {}",
                                    self.ty_display(&right_type)
                                ),
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
                        if !right_type.is_unknown()
                            && !matches!(
                                right_type,
                                Type::Integer(_)
                                    | Type::Long(_)
                                    | Type::UInt(_)
                                    | Type::ULong(_)
                                    | Type::ISize(_)
                                    | Type::USize(_)
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
            _ => crate::internal_error!("non-operators expression reached operators analysis"),
        }
    }
}
