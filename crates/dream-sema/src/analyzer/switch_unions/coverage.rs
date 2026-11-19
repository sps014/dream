//! Switch exhaustiveness as pattern-matrix usefulness: a switch is exhaustive when no value
//! escapes every unguarded arm. A column is split by constructor only when its patterns name every
//! constructor of a finite type (enum variants, `true`/`false`, a tuple); otherwise only the rows
//! that match anything in that column carry on. Splitting consumes a written pattern each time,
//! so the walk stays finite over recursive enums.

use super::*;
use crate::union_table::UnionInfo;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{PatternNode, SwitchArm, Type};

/// One arm's remaining patterns, one per column; `None` matches anything.
type Row<'p> = Vec<Option<&'p PatternNode>>;

enum Ctor {
    Variant(String, Vec<Type>),
    Bool(bool),
    Tuple(Vec<Type>),
}

impl Ctor {
    fn fields(&self) -> &[Type] {
        match self {
            Ctor::Variant(_, fields) | Ctor::Tuple(fields) => fields,
            Ctor::Bool(_) => &[],
        }
    }
}

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn check_exhaustiveness(
        &self,
        subject_base: &str,
        subject_type: &Type,
        union_info: &Option<UnionInfo>,
        arms: &[SwitchArm<'a>],
        position: Option<dream_text::text_span::TextSpan>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if subject_type.is_unknown() {
            return;
        }
        // A guard may fail at runtime, so only unguarded arms cover values.
        let rows: Vec<Row> = arms
            .iter()
            .filter(|a| a.guard.is_none())
            .map(|a| vec![Some(&a.pattern)])
            .collect();
        if let Some(info) = union_info {
            let missing: Vec<&str> = info
                .variants
                .iter()
                .filter(|v| {
                    let ctor = Ctor::Variant(
                        v.name.clone(),
                        v.fields.iter().map(|f| f.type_.clone()).collect(),
                    );
                    let rows = self.specialize(Self::expand_or(rows.clone()), &ctor, subject_type);
                    !self.exhaustive(rows, ctor.fields())
                })
                .map(|v| v.name.as_str())
                .collect();
            if !missing.is_empty() {
                diagnostics.report_error(
                    format!(
                        "Non-exhaustive switch on '{}': missing variant(s) {}. Add the missing arm(s) or a `_` arm",
                        subject_base,
                        missing.join(", ")
                    ),
                    position,
                );
            }
            return;
        }
        if self.exhaustive(rows, std::slice::from_ref(subject_type)) {
            return;
        }
        let message = if subject_type.is_bool() {
            "Non-exhaustive switch on 'bool': cover both `true` and `false`, or add a `_` arm"
                .to_string()
        } else {
            format!("Non-exhaustive switch on '{subject_base}': add a `_` arm to cover all cases")
        };
        diagnostics.report_error(message, position);
    }

    /// True when every value of the column types `tys` matches some row.
    fn exhaustive(&self, rows: Vec<Row>, tys: &[Type]) -> bool {
        let Some((first, rest)) = tys.split_first() else {
            return !rows.is_empty();
        };
        let rows = Self::expand_or(rows);
        if let Some(ctors) = self.constructors(first)
            && ctors
                .iter()
                .all(|c| rows.iter().any(|r| self.names_ctor(r[0], c, first)))
        {
            return ctors.iter().all(|c| {
                let mut columns = c.fields().to_vec();
                columns.extend_from_slice(rest);
                self.exhaustive(self.specialize(rows.clone(), c, first), &columns)
            });
        }
        let defaults: Vec<Row> = rows
            .into_iter()
            .filter(|r| r[0].is_none_or(|p| self.pattern_is_irrefutable(p, first)))
            .map(|r| r[1..].to_vec())
            .collect();
        self.exhaustive(defaults, rest)
    }

    /// The constructors of a type with finitely many, or `None` for numbers, strings, and
    /// classes.
    fn constructors(&self, ty: &Type) -> Option<Vec<Ctor>> {
        if ty.is_bool() {
            return Some(vec![Ctor::Bool(true), Ctor::Bool(false)]);
        }
        if let Type::Tuple(items) = ty {
            return Some(vec![Ctor::Tuple(items.clone())]);
        }
        let info = self
            .type_ctx
            .lookup_type(ty)
            .and_then(|id| self.union_info(id))?;
        Some(
            info.variants
                .iter()
                .map(|v| {
                    Ctor::Variant(
                        v.name.clone(),
                        v.fields.iter().map(|f| f.type_.clone()).collect(),
                    )
                })
                .collect(),
        )
    }

    /// True when `head` is a pattern written for exactly the constructor `ctor`.
    fn names_ctor(&self, head: Option<&PatternNode>, ctor: &Ctor, ty: &Type) -> bool {
        match (head, ctor) {
            (Some(PatternNode::Variant(_, name, _)), Ctor::Variant(v, _)) => name.text == *v,
            (Some(PatternNode::Binding(name)), Ctor::Variant(v, fields)) => {
                fields.is_empty()
                    && name.text == *v
                    && !self.pattern_is_irrefutable_binding(name, ty)
            }
            (Some(PatternNode::Literal(Type::Boolean(t))), Ctor::Bool(value)) => {
                (t.text == "true") == *value
            }
            (Some(PatternNode::Tuple(_)), Ctor::Tuple(_)) => true,
            _ => false,
        }
    }

    /// The rows that can match `ctor`, with its fields replacing the first column.
    fn specialize<'p>(&self, rows: Vec<Row<'p>>, ctor: &Ctor, ty: &Type) -> Vec<Row<'p>> {
        let arity = ctor.fields().len();
        rows.into_iter()
            .filter_map(|row| {
                let head = row[0];
                let subs: Vec<Option<&PatternNode>> = match head {
                    None => vec![None; arity],
                    Some(p) if self.pattern_is_irrefutable(p, ty) => vec![None; arity],
                    Some(p @ (PatternNode::Variant(_, _, subs) | PatternNode::Tuple(subs)))
                        if self.names_ctor(Some(p), ctor, ty) =>
                    {
                        // A wrong arity was already reported; padding keeps it from also
                        // reading as a missing case.
                        (0..arity).map(|i| subs.get(i)).collect()
                    }
                    Some(p) if self.names_ctor(Some(p), ctor, ty) => vec![None; arity],
                    Some(_) => return None,
                };
                let mut next = subs;
                next.extend_from_slice(&row[1..]);
                Some(next)
            })
            .collect()
    }

    /// Splits or-patterns in the first column into one row per alternative.
    fn expand_or(rows: Vec<Row>) -> Vec<Row> {
        let mut out = Vec::with_capacity(rows.len());
        let mut pending: Vec<Row> = rows.into_iter().rev().collect();
        while let Some(row) = pending.pop() {
            match row.first().copied().flatten() {
                Some(PatternNode::Or(alts)) => {
                    for alt in alts.iter().rev() {
                        let mut split = row.clone();
                        split[0] = Some(alt);
                        pending.push(split);
                    }
                }
                _ => out.push(row),
            }
        }
        out
    }

    /// True when `p` always matches a value of type `ty`: `_`, a binding that names no unit
    /// variant of `ty`, or a tuple of such patterns.
    pub(super) fn pattern_is_irrefutable(&self, p: &PatternNode, ty: &Type) -> bool {
        match p {
            PatternNode::Wildcard(_) => true,
            PatternNode::Binding(name) => self.pattern_is_irrefutable_binding(name, ty),
            PatternNode::Tuple(elems) => match ty {
                Type::Tuple(ts) => {
                    ts.len() == elems.len()
                        && elems
                            .iter()
                            .zip(ts)
                            .all(|(p, t)| self.pattern_is_irrefutable(p, t))
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn pattern_is_irrefutable_binding(
        &self,
        name: &dream_syntax::token::syntax_token::SyntaxToken,
        ty: &Type,
    ) -> bool {
        !self
            .type_ctx
            .lookup_type(ty)
            .and_then(|id| self.union_info(id))
            .and_then(|info| info.variant(&name.text))
            .is_some_and(|v| v.fields.is_empty())
    }
}
