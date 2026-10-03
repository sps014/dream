//! The C-style `switch`/`case` statement over int/string/bool/enum subjects (distinct from the
//! pattern-matching `switch` in [`super::super::switch_unions`]).

use super::*;
use crate::errors::SemanticError;
use dream_diagnostics::DiagnosticBag;
use dream_hir::HExpr;
use dream_syntax::nodes::{ExpressionNode, StatementNode, Type};

impl<'a> Analyzer<'a> {
    fn const_case_key(
        &self,
        e: &dream_hir::HExpr,
        layouts: Option<&dream_hir::LayoutTable>,
    ) -> Option<String> {
        match &e.kind {
            dream_hir::HExprKind::SizeOf(ty) => {
                layouts.map(|l| l.size_align(&self.type_ctx.interner, *ty).0.to_string())
            }
            dream_hir::HExprKind::IntLit(v) | dream_hir::HExprKind::EnumValue(v) => {
                Some(v.to_string())
            }
            dream_hir::HExprKind::BoolLit(v) => Some(v.to_string()),
            dream_hir::HExprKind::CharLit(c) => Some((*c as u32).to_string()),
            dream_hir::HExprKind::StringLit(s) => Some(s.clone()),
            dream_hir::HExprKind::FloatLit(f) => Some(f.to_string()),
            dream_hir::HExprKind::Unary {
                op: dream_hir::UnOp::Neg,
                operand,
                ..
            } => {
                if let dream_hir::HExprKind::IntLit(0) = operand.kind {
                    Some("0".to_string())
                } else if let dream_hir::HExprKind::FloatLit(f) = operand.kind {
                    if f == 0.0 {
                        Some("0".to_string())
                    } else {
                        Some(format!("-{f}"))
                    }
                } else {
                    self.const_case_key(operand, layouts)
                        .map(|s| format!("-{s}"))
                }
            }
            _ => None,
        }
    }

    pub(in crate::analyzer) fn validate_layout_case_labels(
        &mut self,
        layouts: &dream_hir::LayoutTable,
        diagnostics: &mut DiagnosticBag,
    ) {
        for labels in std::mem::take(&mut self.deferred_case_labels) {
            let mut seen = indexmap::IndexSet::new();
            for (label, span) in labels {
                if let Some(key) = self.const_case_key(&label, Some(layouts)) {
                    if !seen.insert(key.clone()) {
                        diagnostics.report_error(
                            format!("duplicate case label '{}' in switch statement", key),
                            span,
                        );
                    }
                }
            }
        }
    }

    /// The same constant-label key, read off the AST instead of HIR.
    ///
    /// Shader bodies are emitted straight from the AST and produce no HIR, so `const_case_key`
    /// has nothing to inspect there and every label would look non-constant. This accepts the
    /// shapes a shader can actually use as a label: literals, a negated literal, and an enum
    /// member. Enum members resolve to their value, which is what makes the duplicate check work
    /// across `case 1:` and `case Color.Red:` naming the same number.
    fn const_case_key_ast(&self, e: &ExpressionNode<'_>) -> Option<String> {
        match e {
            ExpressionNode::Literal(ty) => match ty {
                Type::Integer(t) | Type::Boolean(t) | Type::String(t) => Some(t.text.clone()),
                Type::Char(t) => Some((t.text.chars().next()? as u32).to_string()),
                Type::Float(t) | Type::Double(t) => Some(t.text.clone()),
                _ => None,
            },
            ExpressionNode::Parenthesized(_, inner) => self.const_case_key_ast(inner),
            ExpressionNode::Unary(op, inner)
                if op.kind == dream_syntax::token::token_kind::TokenKind::MinusToken =>
            {
                let key = self.const_case_key_ast(inner)?;
                Some(match key.strip_prefix('-') {
                    Some(rest) => rest.to_string(),
                    None if key == "0" => key,
                    None => format!("-{key}"),
                })
            }
            ExpressionNode::MemberAccess(base, member) => {
                let ExpressionNode::Identifier(enum_name) = base else {
                    return None;
                };
                self.enum_members(self.type_ctx.resolve(DefKind::Enum, &enum_name.text)?)?
                    .get(&member.text)
                    .map(|v| v.to_string())
            }
            _ => None,
        }
    }

    pub(in crate::analyzer) fn analyze_case_switch(
        &mut self,
        subject: &ExpressionNode<'a>,
        cases: &Vec<(Vec<ExpressionNode<'a>>, &'a [StatementNode<'a>])>,
        default_body: &Option<&'a [StatementNode<'a>]>,
        ctx: &super::super::AnalyzerContext<'a, '_>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let subject_type = self
            .analyze_expression(subject, ctx.parent_function, ctx.symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let subject_hir = self.hir_take();
        let mut hir_arms: Vec<dream_hir::HArm> = Vec::new();
        // A multi-label case (`case 1, 2, 3:`) becomes one `HArm` per label, all sharing a clone of
        // the case body (each label is a distinct dispatch target hitting the same code).
        let mut hir_ok = true;
        let subject_id = self.type_ctx.lower(&subject_type);
        if !matches!(
            self.type_ctx.interner.kind(subject_id),
            dream_types::TyKind::Prim(
                dream_types::PrimTy::Int | dream_types::PrimTy::String | dream_types::PrimTy::Bool
            ) | dream_types::TyKind::Enum(_)
                | dream_types::TyKind::Error
        ) {
            diagnostics.report_error(
                format!(
                    "switch subject must be int, string, bool, or an enum, got {}",
                    self.type_id_display(subject_id)
                ),
                subject.position(),
            );
        }

        let mut seen: indexmap::IndexSet<String> = indexmap::IndexSet::new();
        let mut deferred_labels = Vec::new();
        let mut needs_layout = false;
        for (labels, body) in cases.iter() {
            let mut label_hirs: Vec<Option<HExpr>> = Vec::new();
            for label in labels.iter() {
                let label_type = self
                    .analyze_expression(label, ctx.parent_function, ctx.symbol_table, diagnostics)
                    .unwrap_or(Type::Unknown);
                let label_hir = self.hir_take();

                // Labels must be compile-time constants: a literal, negative literal, or (for enum switches) an
                // enum member access like `Color.Red`. `analyze_expression` evaluates these to pure HIR
                // constants. If it evaluates to a non-constant (e.g. a runtime field access), reject it.
                let key = match &label_hir {
                    Some(hir) => self.const_case_key(hir, None),
                    None => self.const_case_key_ast(label),
                };

                let layout_constant = label_hir.as_ref().is_some_and(case_needs_layout);
                needs_layout |= layout_constant;
                if let Some(hir) = &label_hir {
                    deferred_labels.push((hir.clone(), label.position()));
                }
                if key.is_none() && !layout_constant && !label_type.is_unknown() {
                    diagnostics.report_error(
                        "switch case labels must be constant literals or enum members".to_string(),
                        label.position(),
                    );
                }

                label_hirs.push(label_hir);
                self.compare_data_type(
                    &subject_type,
                    &label_type,
                    &label.position().unwrap_or_else(empty_span),
                    diagnostics,
                )?;

                if let Some(k) = key {
                    if !seen.insert(k.clone()) {
                        diagnostics.report_error(
                            format!("duplicate case label '{}' in switch statement", k),
                            label.position(),
                        );
                    }
                }
            }
            self.hir_open_block();
            self.analyze_body(
                body,
                ctx.parent_function,
                Some(ctx.symbol_table),
                has_parent_while,
                diagnostics,
            )?;
            let body_hir = self.hir_close_block();
            // One arm per label; all labels of a case share (a clone of) its body.
            for label_hir in label_hirs {
                match self.hir_const_arm(label_hir, body_hir.clone()) {
                    Some(arm) => hir_arms.push(arm),
                    None => hir_ok = false,
                }
            }
        }

        let default_hir = if let Some(db) = default_body {
            self.hir_open_block();
            self.analyze_body(
                db,
                ctx.parent_function,
                Some(ctx.symbol_table),
                has_parent_while,
                diagnostics,
            )?;
            self.hir_close_block()
        } else {
            Vec::new()
        };

        if needs_layout {
            self.deferred_case_labels.push(deferred_labels);
        }
        self.hir_switch(subject_hir, hir_arms, default_hir, hir_ok);
        Ok(())
    }
}

fn case_needs_layout(e: &HExpr) -> bool {
    match &e.kind {
        dream_hir::HExprKind::SizeOf(_) => true,
        dream_hir::HExprKind::Unary {
            op: dream_hir::UnOp::Neg,
            operand,
            ..
        } => case_needs_layout(operand),
        _ => false,
    }
}
