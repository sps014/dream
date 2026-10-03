use super::*;

impl<'a> Analyzer<'a> {
    /// Records the HIR for an enum-member reference (`Enum.Member`) resolved to its integer value.
    pub(in crate::analyzer) fn hir_set_enum_value(&mut self, value: i64, enum_ty: &Type) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let ty = self.type_ctx.lower(enum_ty);
        self.hir.last = Some(HExpr::new(ty, HExprKind::EnumValue(value)));
    }

    /// Records the HIR for a struct field read `obj.field`; `field` is the resolved field index
    /// (declaration order). Clears `last` if the receiver was not representable.
    pub(in crate::analyzer) fn hir_set_field(
        &mut self,
        obj: Option<HExpr>,
        field: usize,
        field_ty: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        self.hir.last = obj.map(|obj| {
            let ty = self.type_ctx.lower(field_ty);
            HExpr::new(
                ty,
                HExprKind::Field {
                    obj: Box::new(obj),
                    field,
                },
            )
        });
    }

    /// Records the HIR for a constructor call `Struct(args)`. `name` is the source (base) struct name
    /// — the registered `DefId` for both plain and generic structs — and `result_ty` supplies the
    /// per-instance layout key. `ctor`, when `Some`, is the resolved user `constructor(){}` def (its
    /// `args` are the constructor's arguments); when `None`, the implicit zero-arg default
    /// constructor takes no args and every field is zero-initialized.
    /// Unresolved names or a non-representable argument drop the call out of coverage.
    pub(in crate::analyzer) fn hir_set_new(
        &mut self,
        name: &str,
        ctor: Option<DefId>,
        args: Vec<Option<HExpr>>,
        result_ty: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let Some(def) = self.type_ctx.resolve(DefKind::Struct, name) else {
            self.hir.last = None;
            return;
        };
        let Some(collected) = Self::collect_hir_args(args) else {
            self.hir.last = None;
            return;
        };
        let ty = self.type_ctx.lower(result_ty);
        let take_params = self.ctor_take_params(ctor);
        self.hir.last = Some(HExpr::new(
            ty,
            HExprKind::New {
                def,
                instance: vec![],
                ctor,
                args: collected,
                take_params,
            },
        ));
    }

    /// Records a resolved instance method call `receiver.method(args)`. `mangled` is the registered
    /// `{Type}_{method}` name; if it does not resolve to a `DefId`, or the receiver/any argument is
    /// not representable, the call drops out of coverage.
    pub(in crate::analyzer) fn hir_set_method_call(
        &mut self,
        receiver: Option<HExpr>,
        identity: &crate::function_table::FunctionIdentity,
        args: Vec<Option<HExpr>>,
        ret: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let Some(receiver) = receiver else {
            self.hir.last = None;
            return;
        };
        let Some(collected) = Self::collect_hir_args(args) else {
            self.hir.last = None;
            return;
        };
        let ret_ty = self.type_ctx.lower(ret);
        let take_params = self.function_table.functions.get(identity).map(|info| info.is_take.clone()).unwrap_or_default();
        let callee = Callee {
            def: identity.0,
            instance: identity.1.clone(),
            ret: ret_ty,
            take_params,
        };
        self.hir.last = Some(HExpr::new(
            ret_ty,
            HExprKind::MethodCall {
                receiver: Box::new(receiver),
                callee,
                args: collected,
            },
        ));
    }

    /// Wraps the last-emitted expression in a logical negation (`!expr`), preserving its type. Used
    /// to lower `a != b` after it has been rewritten to the `equals` call `a.equals(b)`.
    pub(in crate::analyzer) fn hir_negate_last(&mut self) {
        if let Some(expr) = self.hir.last.take() {
            let ty = expr.ty;
            self.hir.last = Some(HExpr::new(
                ty,
                HExprKind::Unary {
                    op: dream_hir::UnOp::Not,
                    operand: Box::new(expr),
                    overflow: dream_hir::Overflow::Wrapping,
                },
            ));
        }
    }

    /// Records a dynamically-dispatched interface method call. `iface` is the interface's `DefId`
    /// and `method_slot` the method's local index within the interface; the backend uses the
    /// receiver's runtime tag to select the concrete implementation. Drops out of coverage if the
    /// receiver or any argument is not representable.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::analyzer) fn hir_set_interface_call(
        &mut self,
        receiver: Option<HExpr>,
        iface_id: usize,
        method_slot: usize,
        sig: TypeId,
        args: Vec<Option<HExpr>>,
        ret: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let Some(receiver) = receiver else {
            self.hir.last = None;
            return;
        };
        let Some(collected) = Self::collect_hir_args(args) else {
            self.hir.last = None;
            return;
        };
        let ret_ty = self.type_ctx.lower(ret);
        self.hir.last = Some(HExpr::new(
            ret_ty,
            HExprKind::InterfaceCall {
                receiver: Box::new(receiver),
                iface_id,
                method_slot,
                sig,
                args: collected,
            },
        ));
    }

    /// Records a discriminated-union construction `Enum.Variant(args)`. `def` is the union's `DefId`
    /// and `variant` its discriminant; any non-representable argument drops it out of coverage.
    pub(in crate::analyzer) fn hir_set_union_new(
        &mut self,
        def: DefId,
        variant: usize,
        args: Vec<Option<HExpr>>,
        result_ty: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let Some(collected) = Self::collect_hir_args(args) else {
            self.hir.last = None;
            return;
        };
        let ty = self.type_ctx.lower(result_ty);
        self.hir.last = Some(HExpr::new(
            ty,
            HExprKind::UnionNew {
                def,
                variant,
                args: collected,
            },
        ));
    }
}
