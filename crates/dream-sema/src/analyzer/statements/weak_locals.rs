use super::*;
use dream_hir::HExpr;
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_types::{DefKind, TyKind};

const WEAK_STORAGE: &str = "system.core::Weak";

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn weak_storage_type(target: &Type) -> Type {
        Self::boxed_type(WEAK_STORAGE, target)
    }

    pub(in crate::analyzer) fn weak_storage_target(&mut self, storage: &Type) -> Option<Type> {
        let ty = self.type_ctx.lower(storage);
        let TyKind::Struct(def, args) = self.type_ctx.interner.kind(ty) else {
            return None;
        };
        if args.len() != 1 || Some(*def) != self.type_ctx.resolve(DefKind::Struct, WEAK_STORAGE) {
            return None;
        }
        Some(self.type_ctx.syntax_type(args[0]))
    }

    pub(in crate::analyzer) fn optional_local_type(target: &Type) -> Type {
        Self::boxed_type("Option", target)
    }

    pub(in crate::analyzer) fn prepare_weak_local(
        &mut self,
        annotation: Option<&Type>,
        actual: &Type,
        mut value: Option<HExpr>,
        name: &SyntaxToken,
        diagnostics: &mut DiagnosticBag,
    ) -> (Type, Option<HExpr>) {
        let annotated_target = annotation
            .map(|t| Self::collection_generic_arg(t, "Option").unwrap_or_else(|| t.clone()));
        let target = annotated_target.unwrap_or_else(|| {
            Self::collection_generic_arg(actual, "Option").unwrap_or_else(|| actual.clone())
        });
        let target_id = self.type_ctx.lower(&target);
        if !target.is_unknown() && self.struct_info(target_id).is_none_or(|info| info.is_value) {
            diagnostics.report_error(
                format!(
                    "'weak' local '{}' must target a class, got '{}'",
                    name.text,
                    self.ty_display(&target)
                ),
                Some(name.position),
            );
            self.hir_fail();
            return (Type::Unknown, None);
        }
        if target.is_unknown() || actual.is_unknown() {
            self.hir_fail();
            return (Type::Unknown, None);
        }
        let optional = Self::optional_local_type(&target);
        let optional_id = self.type_ctx.lower(&optional);
        self.ensure_union_instantiated(
            "Option",
            std::slice::from_ref(&target),
            &name.position,
            diagnostics,
        );
        let actual_target = Self::collection_generic_arg(actual, "Option");
        let compatible = self.compare_data_type(
            &target,
            actual_target.as_ref().unwrap_or(actual),
            &name.position,
            diagnostics,
        );
        if compatible.is_err() {
            self.hir_fail();
            return (Type::Unknown, None);
        }
        if actual_target.is_none() {
            let TyKind::Union(def, _) = self.type_ctx.interner.kind(optional_id) else {
                self.hir_fail();
                return (Type::Unknown, None);
            };
            self.hir_set_union_new(*def, 0, vec![value], &optional);
            value = self.hir_take();
        }
        let storage = Self::weak_storage_type(&target);
        let value = value.and_then(|value| {
            self.hir_build_boxed_new(WEAK_STORAGE, storage.clone(), &target, value)
        });
        (storage, value)
    }

    pub(in crate::analyzer) fn read_weak_local(&mut self, storage: &Type) -> Option<Type> {
        let target = self.weak_storage_target(storage)?;
        let result = Self::optional_local_type(&target);
        let owner = self.type_ctx.lower(storage);
        let receiver = self.hir_take();
        self.hir_set_type_method_call(owner, "get", vec![receiver], &result);
        Some(result)
    }
}
