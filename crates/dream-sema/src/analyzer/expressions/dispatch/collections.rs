use super::*;

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_collections_expression(
        &mut self,
        expression: &ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        match expression {
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
                        self.reject_ref_struct_array_element(
                            &elem,
                            Some(open.position),
                            diagnostics,
                        );
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
                self.current_expected_type = expected_elem.clone();
                let first_type = self.analyze_expression(
                    &elements[0],
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                let mut elem_hirs = vec![self.hir_take()];

                // An array-typed context fixes the element type, so `let xs: object[] = ["a"]` is
                // an `object[]` rather than a `string[]` that then fails to convert — arrays are
                // not covariant, so taking the first element's type would strand the literal.
                // Without such a context the first element sets it, and later elements must match.
                let elem_ty = match &expected_elem {
                    Some(expected) => {
                        let span = elements[0].position().unwrap_or(open.position);
                        self.compare_data_type(expected, &first_type, &span, diagnostics)?;
                        expected.clone()
                    }
                    None => first_type,
                };

                for elem in elements.iter().skip(1) {
                    let element_type =
                        self.analyze_expression(elem, parent_function, symbol_table, diagnostics)?;
                    elem_hirs.push(self.hir_take());
                    let span = elem.position().unwrap_or(open.position);
                    self.compare_data_type(&elem_ty, &element_type, &span, diagnostics)?;
                }
                self.current_expected_type = saved_expected;

                self.reject_ref_struct_array_element(&elem_ty, Some(open.position), diagnostics);
                let array_type = Type::Array(Box::new(elem_ty));
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
                    self.reject_ref_struct_array_element(
                        &elem_ty,
                        Some(open.position),
                        diagnostics,
                    );
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
            _ => crate::internal_error!("non-collections expression reached collections analysis"),
        }
    }
}
