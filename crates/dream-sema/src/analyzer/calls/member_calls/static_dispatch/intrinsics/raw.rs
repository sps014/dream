use super::*;
use dream_hir::{HExpr, HExprKind};

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_raw_intrinsic(
        &mut self,
        call: &GenericStaticMethodCall<'a, '_>,
        types: Vec<dream_types::TypeId>,
        hirs: Vec<Option<HExpr>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let store = intrinsics::IntrinsicOp::from_attributes(&call.template.attributes)
            == Some(intrinsics::IntrinsicOp::RawStore);
        self.check_unsafe_intrinsic_call(
            "raw memory access",
            call.template,
            call.method.position,
            diagnostics,
        );
        let Some(args) = call.generic_args.as_ref().filter(|args| args.len() == 1) else {
            diagnostics.report_error(
                "raw memory access requires one unmanaged type argument".to_string(),
                Some(call.method.position),
            );
            return Ok(Type::Unknown);
        };
        let target = Self::monomorphize_type(&args[0], &self.current_generic_bindings);
        if !self.type_satisfies_kind(&target, dream_syntax::nodes::ConstraintKind::Unmanaged) {
            self.require_unmanaged(
                &target,
                "raw memory access",
                &call.method.position,
                diagnostics,
            );
            return Ok(Type::Unknown);
        }
        let expected = if store { 2 } else { 1 };
        if types.len() != expected
            || types.first() != Some(&self.type_ctx.interner.prim(dream_types::PrimTy::USize))
        {
            diagnostics.report_error(
                format!("raw memory access expects {expected} arguments and a usize address"),
                Some(call.method.position),
            );
            return Ok(Type::Unknown);
        }
        let ty = self.type_ctx.lower(&target);
        if matches!(self.type_ctx.interner.kind(ty), dream_types::TyKind::Error)
            && !self.is_abstract_generic_type(&target)
        {
            diagnostics.report_error(
                "raw memory access requires a known element type".to_string(),
                Some(call.method.position),
            );
            return Ok(Type::Unknown);
        }
        if types
            .iter()
            .any(|&ty| matches!(self.type_ctx.interner.kind(ty), dream_types::TyKind::Error))
        {
            return Ok(Type::Unknown);
        }
        if store && !dream_types::assignable(&self.type_ctx.interner, ty, types[1]) {
            diagnostics.report_error(
                "raw store value does not match its element type".to_string(),
                Some(call.method.position),
            );
            return Ok(Type::Unknown);
        }
        let mut hirs = hirs.into_iter();
        let address = hirs.next().flatten();
        let value = hirs.next().flatten();
        let expr = match (address, value, store) {
            (Some(address), Some(value), true) => {
                let value = if value.ty == ty {
                    value
                } else {
                    HExpr::new(ty, HExprKind::Cast(Box::new(value)))
                };
                Some(HExpr::new(
                    self.type_ctx.interner.void(),
                    HExprKind::RawStore {
                        address: Box::new(address),
                        value: Box::new(value),
                    },
                ))
            }
            (Some(address), _, false) => {
                Some(HExpr::new(ty, HExprKind::RawLoad(Box::new(address))))
            }
            _ => None,
        };
        self.hir_set_last(expr);
        Ok(if store { Type::Void } else { target })
    }
}
