use super::args::{CallArgAnalysis, NamedSlot};
use super::*;
use crate::function_table::{FunctionIdentity, FunctionTableInfo};
use dream_types::{TyKind, TypeId};

pub(super) struct NamedCall<'a> {
    pub signature: FunctionTableInfo,
    pub expressions: Vec<ExpressionNode<'a>>,
    pub arguments: CallArgAnalysis,
}

struct Candidate {
    signature: FunctionTableInfo,
    slots: Vec<NamedSlot>,
}

impl Candidate {
    fn argument_types(
        &self,
        args: &CallArgAnalysis,
        skip: usize,
        interner: &dream_types::TypeInterner,
    ) -> Option<Vec<(TypeId, TypeId)>> {
        let mut pairs = Vec::new();
        for (i, slot) in self.slots.iter().enumerate() {
            let parameter = i + skip;
            let expected = *self.signature.parameters.get(parameter)?;
            match slot {
                NamedSlot::Source(index) => {
                    if args.2[*index] != self.signature.is_ref[parameter]
                        || (args.2[*index] && expected != args.0[*index])
                    {
                        return None;
                    }
                    pairs.push((expected, args.0[*index]));
                }
                NamedSlot::Default(_) => {}
                NamedSlot::Variadic(indices) => {
                    let TyKind::Array(element) = interner.kind(expected) else {
                        return None;
                    };
                    for &index in indices {
                        if args.2[index] {
                            return None;
                        }
                        pairs.push((*element, args.0[index]));
                    }
                }
            }
        }
        Some(pairs)
    }
}

impl<'a> Analyzer<'a> {
    pub(super) fn resolve_named_call(
        &mut self,
        callee: (&str, TextSpan),
        keys: Vec<FunctionIdentity>,
        raw: &[ExpressionNode<'a>],
        skip: usize,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<NamedCall<'a>, SemanticError> {
        let (name, position) = callee;
        let runs_on_worker = self.candidates_run_on_worker(&keys);
        let mut candidates = Vec::new();
        let mut layout_error = None;
        for key in keys {
            let Ok(signature) = self.function_table.get_function(&key) else {
                continue;
            };
            let names = signature.param_names.get(skip..).unwrap_or_default();
            let defaults = signature.defaults.get(skip..).unwrap_or_default();
            match Self::try_normalize_named_arguments(
                names,
                defaults,
                raw,
                position,
                signature.is_variadic,
            ) {
                Ok(slots) => candidates.push(Candidate { signature, slots }),
                Err(error) => layout_error = Some(error),
            }
        }
        if candidates.is_empty() {
            let (message, span) = layout_error.unwrap_or_else(|| {
                (
                    format!("named arguments are not supported for '{name}'"),
                    Some(position),
                )
            });
            return Err(report(diagnostics, message, span));
        }

        let source: Vec<_> = (0..raw.len())
            .map(|index| NamedSlot::Source(index).expression(raw))
            .collect();
        let expected = self.named_argument_context(&candidates, &source, skip);
        let saved_target = self.enter_call_target(name.to_owned(), runs_on_worker);
        let result = self.analyze_call_arguments_expecting_ref(
            &source,
            Some(&expected),
            ctx.parent_function,
            ctx.symbol_table,
            diagnostics,
        );
        self.current_call_target = saved_target;
        let mut args = result?;

        let mut best = None;
        let mut tied = false;
        for candidate in candidates {
            let Some(pairs) = candidate.argument_types(&args, skip, &self.type_ctx.interner) else {
                continue;
            };
            let mut sink = DiagnosticBag::new(None);
            let score = pairs.into_iter().try_fold(0, |score, (expected, actual)| {
                if expected == actual {
                    Some(score + 1)
                } else if dream_types::overload_compatible(
                    &self.type_ctx.interner,
                    expected,
                    actual,
                ) || self.value_type_assignable(expected, actual, &mut sink)
                {
                    Some(score)
                } else {
                    None
                }
            });
            let Some(score) = score else { continue };
            let uses_default = candidate.slots.len() + skip < candidate.signature.parameters.len()
                || candidate
                    .slots
                    .iter()
                    .any(|slot| matches!(slot, NamedSlot::Default(_)));
            let score = score + i32::from(!uses_default);
            match best.as_ref().map(|(best_score, _)| score.cmp(best_score)) {
                None | Some(std::cmp::Ordering::Greater) => {
                    best = Some((score, candidate));
                    tied = false;
                }
                Some(std::cmp::Ordering::Equal) => tied = true,
                Some(std::cmp::Ordering::Less) => {}
            }
        }
        let Some((_, candidate)) = best else {
            let types = args
                .0
                .iter()
                .map(|ty| self.type_id_display(*ty))
                .collect::<Vec<_>>();
            return Err(report(
                diagnostics,
                format!(
                    "No overload of '{name}' matches argument types ({})",
                    types.join(", ")
                ),
                Some(position),
            ));
        };
        if tied {
            return Err(report(
                diagnostics,
                format!("Ambiguous named-argument call to '{name}'"),
                Some(position),
            ));
        }

        let Candidate { signature, slots } = candidate;
        let expressions: Vec<_> = slots.iter().map(|slot| slot.expression(raw)).collect();
        let mut ordered: CallArgAnalysis = (Vec::new(), Vec::new(), Vec::new());
        for (i, slot) in slots.into_iter().enumerate() {
            let parameter = signature.parameters[i + skip];
            let (ty, hir, is_ref) = match slot {
                NamedSlot::Source(index) => (args.0[index], args.1[index].take(), args.2[index]),
                NamedSlot::Variadic(indices) => {
                    let array = self.type_ctx.syntax_type(parameter);
                    let elements = indices
                        .into_iter()
                        .map(|index| args.1[index].take())
                        .collect();
                    self.hir_set_array_lit(elements, &array);
                    (parameter, self.hir_take(), false)
                }
                NamedSlot::Default(_) => {
                    let expected = &signature.parameter_types[i + skip];
                    let (mut types, mut hirs, _) = self.analyze_call_arguments_expecting_ref(
                        &expressions[i..=i],
                        Some(std::slice::from_ref(expected)),
                        ctx.parent_function,
                        ctx.symbol_table,
                        diagnostics,
                    )?;
                    (types.remove(0), hirs.remove(0), false)
                }
            };
            ordered.0.push(ty);
            ordered.1.push(hir);
            ordered.2.push(is_ref);
        }
        Ok(NamedCall {
            signature,
            expressions,
            arguments: ordered,
        })
    }

    fn named_argument_context(
        &self,
        candidates: &[Candidate],
        source: &[ExpressionNode<'a>],
        skip: usize,
    ) -> Vec<Type> {
        let mut hints: Vec<Vec<TypeId>> = vec![Vec::new(); source.len()];
        for candidate in candidates {
            for (i, slot) in candidate.slots.iter().enumerate() {
                let Some(&parameter) = candidate.signature.parameters.get(i + skip) else {
                    continue;
                };
                match slot {
                    NamedSlot::Source(index) => hints[*index].push(parameter),
                    NamedSlot::Variadic(indices) => {
                        if let TyKind::Array(element) = self.type_ctx.interner.kind(parameter) {
                            for &index in indices {
                                hints[index].push(*element);
                            }
                        }
                    }
                    NamedSlot::Default(_) => {}
                }
            }
        }
        hints
            .into_iter()
            .zip(source)
            .map(|(types, expr)| {
                let mut types = types
                    .into_iter()
                    .map(|id| self.type_ctx.syntax_type(id))
                    .filter(|ty| {
                        !Self::is_async_lambda_expr(expr) || Self::is_future_returning_fun(ty)
                    });
                let first = types.next().unwrap_or(Type::Unknown);
                if types.all(|ty| ty == first) {
                    first
                } else {
                    Type::Unknown
                }
            })
            .collect()
    }
}
