//! The C-style `switch`/`case` statement over int/string/bool/enum subjects (distinct from the
//! pattern-matching `switch` in [`super::super::switch_unions`]).

use super::*;
use crate::errors::SemanticError;
use dream_diagnostics::DiagnosticBag;
use dream_hir::HExpr;
use dream_syntax::nodes::{ExpressionNode, StatementNode, Type};

impl<'a> Analyzer<'a> {
    fn const_case_key(&self, e: &dream_hir::HExpr) -> Option<String> {
        match &e.kind {
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
                    self.const_case_key(operand).map(|s| format!("-{s}"))
                }
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
        let subject_name = subject_type.get_type();
        let subject_is_enum = self.enum_table.contains_key(&subject_name);
        if !matches!(subject_name.as_str(), "int" | "string" | "bool") && !subject_is_enum {
            diagnostics.report_error(
                format!(
                    "switch subject must be int, string, bool, or an enum, got {}",
                    self.ty_str_display(&subject_name)
                ),
                subject.position(),
            );
        }

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
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
                let key = if let Some(hir) = &label_hir {
                    self.const_case_key(hir)
                } else {
                    None
                };

                if key.is_none() && !label_type.is_unknown() {
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

        self.hir_switch(subject_hir, hir_arms, default_hir, hir_ok);
        Ok(())
    }
}
