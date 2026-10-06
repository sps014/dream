use super::*;

impl<'a> Analyzer<'a> {
    /// `main`'s return value is the process exit status, so only the spellings the entry point can
    /// act on are accepted: `void` exits 0, `int` *is* the exit code, and `Result<T, E>` reports an
    /// `Err` on stderr and exits 1. Anything else would be silently discarded, so it is rejected.
    pub(in crate::analyzer) fn validate_entry_return_type(
        &mut self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let Some(ret) = function.return_type.clone() else {
            return;
        };
        if matches!(ret, Type::Void | Type::Integer(_) | Type::Unknown) {
            return;
        }
        let parts = Self::resolve_struct_parts(&ret);
        if let Some((base, args)) = &parts
            && base == "Result" && args.len() == 2 {
                return;
            }
        let mut message = format!(
            "'main' must return void, int, or Result<T, E>, got {}",
            self.ty_display(&ret)
        );
        if parts.is_some_and(|(base, _)| base == "Option") {
            message.push_str(" (use 'ok_or' to turn an Option into a Result)");
        }
        diagnostics.report_error(message, Some(function.name.position));
    }

    /// Appends `main`'s implicit tail return (see [`crate::entry`]) to the body currently being
    /// collected, so the entry point always hands the backend an explicit exit status. Callers
    /// invoke this after the body is analyzed and before the function is finished; the HIR helpers
    /// no-op when the function is not an emission candidate.
    pub(in crate::analyzer) fn hir_emit_entry_tail_return(
        &mut self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let Some(tail) = entry_tail_return(function) else {
            return;
        };
        let Some(return_type) = function.return_type.clone() else {
            return;
        };
        let target = self.type_ctx.lower(&return_type);
        match tail {
            EntryTail::Zero => {
                let zero = HExpr::new(target, HExprKind::IntLit(0));
                self.hir_return_value(Some(zero), Some(target));
            }
            EntryTail::OkTrue => {
                let Some((base, args)) = Self::resolve_struct_parts(&return_type) else {
                    return;
                };
                self.ensure_union_instantiated(&base, &args, &function.name.position, diagnostics);
                let def = match self.type_ctx.interner.kind(target) {
                    dream_types::TyKind::Union(def, _) => Some(*def),
                    _ => None,
                };
                let disc = self
                    .union_info(target)
                    .and_then(|u| u.variant("Ok"))
                    .map(|v| v.discriminant as usize);
                let (Some(def), Some(disc)) = (def, disc) else {
                    self.hir_fail();
                    return;
                };
                let payload = HExpr::new(self.type_ctx.interner.bool(), HExprKind::BoolLit(true));
                self.hir_set_union_new(def, disc, vec![Some(payload)], &return_type);
                let value = self.hir_take();
                self.hir_return_value(value, Some(target));
            }
        }
    }
}
