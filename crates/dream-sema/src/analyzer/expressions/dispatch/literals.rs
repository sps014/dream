use super::*;

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_literals_expression(
        &mut self,
        expression: &ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        match expression {
            ExpressionNode::Literal(number) => {
                if let Type::Struct(base, Some(args)) = number
                    && let [Type::Struct(member, None)] = args.as_slice()
                {
                    if let Some(t) = self.analyze_variant_construction(
                        &base.text,
                        member,
                        &[],
                        parent_function,
                        symbol_table,
                        diagnostics,
                    )? {
                        return Ok(t);
                    }
                    if let Some(def) = self
                        .type_ctx
                        .resolve(dream_types::DefKind::Enum, &base.text)
                        .filter(|&def| self.enum_members(def).is_some())
                    {
                        let enum_ty = Type::Struct(base.clone(), None);
                        match self
                            .enum_members(def)
                            .and_then(|members| members.get(&member.text))
                            .copied()
                        {
                            Some(value) => self.hir_set_enum_value(value as i64, &enum_ty),
                            None => {
                                diagnostics.report_error(
                                    format!("Enum '{}' has no member '{}'", base.text, member.text),
                                    Some(member.position),
                                );
                                self.hir_none();
                            }
                        }
                        return Ok(enum_ty);
                    }
                    diagnostics.report_error(
                        format!(
                            "cannot use '{}.{}' as a default value",
                            base.text, member.text
                        ),
                        Some(member.position),
                    );
                    return Ok(Type::Unknown);
                }
                let mut ty =
                    Self::retarget_numeric_literal(number, self.current_expected_type.as_ref());

                if let Type::Integer(t)
                | Type::Long(t)
                | Type::UInt(t)
                | Type::ULong(t)
                | Type::ISize(t)
                | Type::USize(t)
                | Type::Byte(t) = &ty
                {
                    if matches!(ty, Type::ULong(_) | Type::USize(_)) {
                        let parsed = dream_syntax::number::parse_u64_literal(&t.text);
                        if parsed.is_none()
                            || (matches!(ty, Type::USize(_))
                                && self.target_layout.ptr_size == 4
                                && parsed.is_some_and(|v| v > u32::MAX as u64))
                        {
                            diagnostics.report_error(
                                format!(
                                    "integer literal '{}' is out of range or malformed",
                                    t.text
                                ),
                                number.get_span(),
                            );
                        }
                    } else if let Some(val) =
                        dream_syntax::number::parse_signed_literal(&t.text, 64)
                    {
                        let fits_signed = |bits| {
                            dream_syntax::number::parse_signed_literal(&t.text, bits).is_some()
                        };
                        let err = match &ty {
                            Type::Integer(_) => !fits_signed(32),
                            Type::Byte(_) => !(0i64..=255).contains(&val),
                            Type::UInt(_) => !(0..=u32::MAX as i64).contains(&val),
                            Type::Long(_) => false,
                            Type::ISize(_) => self.target_layout.ptr_size == 4 && !fits_signed(32),
                            _ => false,
                        };
                        if err && matches!(ty, Type::Integer(_)) {
                            ty = Type::Long(t.clone());
                        } else if err {
                            diagnostics.report_error(
                                format!("literal {} does not fit in {}", t.text, ty.get_type()),
                                number.get_span(),
                            );
                        }
                    } else {
                        diagnostics.report_error(
                            format!("integer literal '{}' is out of range or malformed", t.text),
                            number.get_span(),
                        );
                    }
                } else if let Type::Float16(t) | Type::Float(t) | Type::Double(t) = &ty {
                    match dream_syntax::number::parse_float_literal(&t.text) {
                        None => diagnostics.report_error(
                            format!("float literal '{}' is out of range or malformed", t.text),
                            number.get_span(),
                        ),
                        Some(v)
                            if matches!(ty, Type::Float16(_))
                                && v.is_finite()
                                && half::f16::from_f64(v).is_infinite() =>
                        {
                            diagnostics.report_error(
                                format!(
                                    "literal {} does not fit in float16 (largest finite value is 65504)",
                                    t.text
                                ),
                                number.get_span(),
                            )
                        }
                        Some(_) => {}
                    }
                }

                self.hir_set_literal(&ty);
                Ok(ty)
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
            _ => crate::internal_error!("non-literals expression reached literals analysis"),
        }
    }
}
