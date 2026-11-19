//! Pattern type-checking for `switch`: checks each pattern against the subject type, introduces
//! its bindings (`check_pattern`), and validates variant payload arity and types. Coverage across
//! arms lives in `coverage`.

use super::*;
use crate::errors::SemanticError;
use crate::symbol_table::SymbolTable;
use crate::union_table::UnionInfo;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{PatternNode, Type};
use std::cell::RefCell;
use std::rc::Rc;

use super::PatternInfo;

impl<'a> Analyzer<'a> {
    /// Type-checks `pattern` against `expected`, introducing any bindings into `scope`.
    pub(in crate::analyzer) fn check_pattern(
        &mut self,
        pattern: &PatternNode,
        expected: &Type,
        scope: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<PatternInfo, SemanticError> {
        let expected_base = self.ty_display(expected);
        let expected_id = self.type_ctx.lower(expected);
        let union_info: Option<UnionInfo> = self.union_info(expected_id).cloned();

        match pattern {
            PatternNode::Wildcard(_) => Ok(PatternInfo { irrefutable: true }),
            PatternNode::Binding(name) => {
                // A bare identifier that names a unit variant of the matched union is a
                // unit-variant pattern; otherwise it binds the whole value.
                if let Some(info) = &union_info
                    && let Some(v) = info.variant(&name.text)
                    && v.fields.is_empty()
                {
                    return Ok(PatternInfo { irrefutable: false });
                }
                if union_info.is_some()
                    && let Some(owner) = self.union_table.values().find(|other| {
                        other
                            .variant(&name.text)
                            .is_some_and(|v| v.fields.is_empty())
                    })
                {
                    diagnostics.report_error(
                        format!(
                            "pattern '{}' is a variant of '{}', not of the matched enum '{}'",
                            name.text, owner.name, expected_base
                        ),
                        Some(name.position),
                    );
                    return Ok(PatternInfo { irrefutable: false });
                }
                if let Err(e) = (*scope)
                    .borrow_mut()
                    .add_symbol(name.text.clone(), expected.clone())
                {
                    diagnostics.report_error(e.to_string(), Some(name.position));
                }
                Ok(PatternInfo { irrefutable: true })
            }
            PatternNode::Literal(lit) => {
                if !lit.is_unknown() && !expected.is_unknown() && {
                    let lit_id = self.type_ctx.lower(lit);
                    !self.value_type_assignable(expected_id, lit_id, diagnostics)
                } {
                    diagnostics.report_error(
                        format!(
                            "Pattern literal of type '{}' cannot match a value of type '{}'",
                            self.ty_display(lit),
                            expected_base
                        ),
                        lit.get_span(),
                    );
                }
                Ok(PatternInfo { irrefutable: false })
            }
            PatternNode::Variant(qualifier, variant, subs) => {
                let info = match &union_info {
                    Some(info) => info.clone(),
                    None => {
                        diagnostics.report_error(
                            format!(
                                "Variant pattern '{}' can only match a discriminated union, not '{}'",
                                variant.text, expected_base
                            ),
                            Some(variant.position),
                        );
                        // Still walk sub-patterns so their bindings/errors surface.
                        for sub in subs {
                            self.check_pattern(sub, &Type::Unknown, scope, diagnostics)?;
                        }
                        return Ok(PatternInfo { irrefutable: false });
                    }
                };

                if let Some(q) = qualifier {
                    let expected_def = match self.type_ctx.interner.kind(expected_id) {
                        dream_types::TyKind::Union(def, _) => Some(*def),
                        _ => None,
                    };
                    if self.type_ctx.resolve(DefKind::Union, &q.text) != expected_def {
                        diagnostics.report_error(
                            format!(
                                "pattern '{}.{}' is a variant of '{}', not of the matched enum '{}'",
                                q.text, variant.text, q.text, expected_base
                            ),
                            Some(q.position),
                        );
                        for sub in subs {
                            self.check_pattern(sub, &Type::Unknown, scope, diagnostics)?;
                        }
                        return Ok(PatternInfo { irrefutable: false });
                    }
                }

                let var_info = match info.variant(&variant.text) {
                    Some(v) => v.clone(),
                    None => {
                        diagnostics.report_error(
                            format!("Enum '{}' has no variant '{}'", expected_base, variant.text),
                            Some(variant.position),
                        );
                        for sub in subs {
                            self.check_pattern(sub, &Type::Unknown, scope, diagnostics)?;
                        }
                        return Ok(PatternInfo { irrefutable: false });
                    }
                };

                if subs.len() != var_info.fields.len() {
                    diagnostics.report_error(
                        format!(
                            "Variant '{}.{}' has {} field(s), but the pattern binds {}",
                            expected_base,
                            variant.text,
                            var_info.fields.len(),
                            subs.len()
                        ),
                        Some(variant.position),
                    );
                }

                // Recurse into each sub-pattern for its own type-checking / binding introduction.
                for (i, sub) in subs.iter().enumerate() {
                    let field_type = var_info
                        .fields
                        .get(i)
                        .map(|f| f.type_.clone())
                        .unwrap_or(Type::Unknown);
                    self.check_pattern(sub, &field_type, scope, diagnostics)?;
                }

                // A variant pattern is refutable on its own; whether the variant is fully covered
                // (across all arms, including nested sub-patterns) is decided in `check_exhaustiveness`.
                Ok(PatternInfo { irrefutable: false })
            }
            PatternNode::Tuple(elems) => match expected {
                Type::Tuple(ts) => {
                    if ts.len() != elems.len() {
                        diagnostics.report_error(
                            format!(
                                "tuple pattern has {} elements but subject type has {}",
                                elems.len(),
                                ts.len()
                            ),
                            pattern.position(),
                        );
                    }
                    let mut irrefutable = true;
                    for (i, sub) in elems.iter().enumerate() {
                        let slot = ts.get(i).cloned().unwrap_or(Type::Unknown);
                        let info = self.check_pattern(sub, &slot, scope, diagnostics)?;
                        irrefutable = irrefutable && info.irrefutable;
                    }
                    Ok(PatternInfo { irrefutable })
                }
                Type::Unknown => {
                    for sub in elems {
                        self.check_pattern(sub, &Type::Unknown, scope, diagnostics)?;
                    }
                    Ok(PatternInfo { irrefutable: false })
                }
                other => {
                    diagnostics.report_error(
                        format!(
                            "tuple pattern cannot match a value of type '{}'",
                            self.ty_display(other)
                        ),
                        pattern.position(),
                    );
                    for sub in elems {
                        self.check_pattern(sub, &Type::Unknown, scope, diagnostics)?;
                    }
                    Ok(PatternInfo { irrefutable: false })
                }
            },
            PatternNode::Range(lo, hi) => {
                let float_subject = matches!(
                    self.type_ctx.interner.kind(expected_id),
                    dream_types::TyKind::Prim(primitive) if primitive.is_float()
                );
                if float_subject && (lo.is_integer() || hi.is_integer()) {
                    diagnostics.report_error(
                        format!(
                            "an integer range cannot match a '{expected_base}' subject; write the bounds as '{expected_base}' literals"
                        ),
                        lo.get_span(),
                    );
                    return Ok(PatternInfo { irrefutable: false });
                }
                for bound in [lo, hi] {
                    if !bound.is_unknown() && !expected.is_unknown() && {
                        let bound_id = self.type_ctx.lower(bound);
                        !self.value_type_assignable(expected_id, bound_id, diagnostics)
                    } {
                        diagnostics.report_error(
                            format!(
                                "Range pattern bound of type '{}' cannot match a value of type '{}'",
                                self.ty_display(bound),
                                expected_base
                            ),
                            bound.get_span(),
                        );
                    }
                }
                if !expected.is_unknown()
                    && !matches!(self.type_ctx.interner.kind(expected_id), dream_types::TyKind::Prim(primitive) if primitive.is_numeric() || *primitive == dream_types::PrimTy::Char)
                {
                    diagnostics.report_error(
                        format!(
                            "Range pattern requires an ordered numeric or char subject, got '{}'",
                            expected_base
                        ),
                        lo.get_span(),
                    );
                }
                Ok(PatternInfo { irrefutable: false })
            }
            PatternNode::Or(alts) => {
                let mut irrefutable_any = false;
                for alt in alts {
                    if Self::pattern_introduces_binding(alt, &union_info) {
                        diagnostics.report_error(
                            "an or-pattern alternative cannot bind a variable; use `_`, a literal, a range, or a payload-free variant".to_string(),
                            alt.position(),
                        );
                        continue;
                    }
                    let info = self.check_pattern(alt, expected, scope, diagnostics)?;
                    irrefutable_any = irrefutable_any || info.irrefutable;
                }
                Ok(PatternInfo {
                    irrefutable: irrefutable_any,
                })
            }
        }
    }
    pub(in crate::analyzer) fn validate_variant_payload(
        &mut self,
        enum_name: &str,
        variant_name: &str,
        field_types: &[Type],
        arg_types: &[Type],
        position: dream_text::text_span::TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        if arg_types.len() != field_types.len() {
            diagnostics.report_error(
                format!(
                    "Variant '{}.{}' expects {} argument(s), but {} were given",
                    enum_name,
                    variant_name,
                    field_types.len(),
                    arg_types.len()
                ),
                Some(position),
            );
        }

        let expected_strs: Vec<_> = field_types.iter().map(|t| self.type_ctx.lower(t)).collect();
        let given_strs: Vec<_> = arg_types.iter().map(|t| self.type_ctx.lower(t)).collect();

        self.validate_arguments(
            &format!("Variant '{}.{}'", enum_name, variant_name),
            &expected_strs,
            &given_strs,
            position,
            &[],
            diagnostics,
        );
    }

    /// True when `p` would introduce at least one variable binding — recursively, so a variant
    /// pattern only counts as binding-free when every one of its sub-patterns is too (e.g.
    /// `Circle(_)` does not bind, but `Circle(r)` does). Used to reject a binding-introducing
    /// or-pattern alternative, since a name bound by only *some* alternatives would be meaningless
    /// (which alternative matched is not visible to the arm body).
    pub(super) fn pattern_introduces_binding(
        p: &PatternNode,
        union_info: &Option<UnionInfo>,
    ) -> bool {
        match p {
            PatternNode::Wildcard(_) | PatternNode::Literal(_) | PatternNode::Range(..) => false,
            PatternNode::Binding(name) => !matches!(union_info, Some(info)
                if info.variant(&name.text).is_some_and(|v| v.fields.is_empty())),
            PatternNode::Variant(_, _, subs) => subs
                .iter()
                .any(|s| Self::pattern_introduces_binding(s, union_info)),
            PatternNode::Tuple(elems) => elems
                .iter()
                .any(|s| Self::pattern_introduces_binding(s, union_info)),
            PatternNode::Or(alts) => alts
                .iter()
                .any(|a| Self::pattern_introduces_binding(a, union_info)),
        }
    }
}
