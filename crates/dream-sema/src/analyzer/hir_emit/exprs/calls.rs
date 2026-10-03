//! HIR for calls (direct/indirect/generic), function values, field/enum reads, constructors, and
//! method/interface/union construction.

use super::*;

mod closures;
mod indirect;
mod members;
impl<'a> Analyzer<'a> {
    /// Per-parameter `take` flags for a function/method registered under `name` (empty if unknown).
    pub(in crate::analyzer) fn take_params_for(&self, name: &str) -> Vec<bool> {
        self.function_table
            .functions
            .get(name)
            .map(|f| f.is_take.clone())
            .unwrap_or_default()
    }

    /// Per-argument `take` flags for a resolved constructor def, aligned with the constructor's user
    /// arguments. A registered constructor signature carries `this` as parameter 0, which the `New`
    /// argument list does not, so that flag is dropped here.
    pub(in crate::analyzer) fn ctor_take_params(&self, ctor: Option<DefId>) -> Vec<bool> {
        let Some(ctor) = ctor else {
            return Vec::new();
        };
        let mut flags = self.take_params_for(self.type_ctx.defs.name(ctor));
        if flags.is_empty() {
            return flags;
        }
        flags.remove(0);
        flags
    }

    /// Records the HIR for a direct free-function call `name(args)`. Resolves `name` to its function
    /// `DefId`; if it is not a registered (non-generic, non-overloaded) function or any argument is
    /// not representable, the call is dropped from HIR coverage (enclosing function may fail
    /// backend support).
    pub(in crate::analyzer) fn hir_set_call(
        &mut self,
        name: &str,
        args: Vec<Option<HExpr>>,
        ret: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let Some(def) = self.type_ctx.resolve(DefKind::Function, name) else {
            self.hir.last = None;
            return;
        };
        let Some(collected) = Self::collect_hir_args(args) else {
            self.hir.last = None;
            return;
        };
        let ret_ty = self.type_ctx.lower(ret);
        let take_params = self.take_params_for(name);
        let callee = Callee {
            def,
            instance: vec![],
            ret: ret_ty,
            take_params,
        };
        self.hir.last = Some(HExpr::new(
            ret_ty,
            HExprKind::Call {
                callee,
                args: collected,
            },
        ));
    }

    /// The `Cell<elem>` struct type wrapping a captured local of type `elem` (see
    /// `src/stdlib/core/closure.dream`) — the shared mutable box every reader of a captured name
    /// (the enclosing function and every closure that captures it) reads/writes through.
    pub(in crate::analyzer) fn cell_type(elem: &Type) -> Type {
        Self::boxed_type("CaptureCell", elem)
    }

    /// The `RefBox<elem>` value-struct type wrapping a local/parameter that is `ref`-passed but
    /// never closure-captured (see `src/stdlib/core/closure.dream` and `docs/compiler/03-hir.md`).
    pub(in crate::analyzer) fn ref_box_type(elem: &Type) -> Type {
        Self::boxed_type("RefBox", elem)
    }

    fn boxed_type(base: &str, elem: &Type) -> Type {
        Type::Struct(
            crate::analyzer::synthetic_token(TokenKind::IdentifierToken, base),
            Some(vec![elem.clone()]),
        )
    }

    /// Builds `CaptureCell<elem_ty>(value)`: ensures that instantiation of the generic `CaptureCell<T>` class
    /// exists (registering it on first use, exactly like an ordinary `List<T>()` construction site),
    /// then emits the `New` HIR wrapping `value`. Used both for a captured `let`/parameter (see
    /// `hir_declare_local`) and — indirectly, by construction at the capturing lambda's use site —
    /// for the environment word handed to `build_funcbox`.
    pub(in crate::analyzer) fn hir_build_cell_new(
        &mut self,
        elem_ty: &Type,
        value: HExpr,
    ) -> Option<HExpr> {
        self.hir_build_boxed_new("CaptureCell", Self::cell_type(elem_ty), elem_ty, value)
    }

    /// Builds `RefBox<elem_ty>(value)`, the same shape as [`Self::hir_build_cell_new`] but backed
    /// by the value-struct box a purely `ref`-passed (not closure-captured) name uses instead of the
    /// heap `CaptureCell<T>` — see `hir_declare_local`/`Analyzer::hir_begin_function`.
    pub(in crate::analyzer) fn hir_build_ref_box_new(
        &mut self,
        elem_ty: &Type,
        value: HExpr,
    ) -> Option<HExpr> {
        self.hir_build_boxed_new("RefBox", Self::ref_box_type(elem_ty), elem_ty, value)
    }

    fn hir_build_boxed_new(
        &mut self,
        base: &str,
        boxed_ty: Type,
        elem_ty: &Type,
        value: HExpr,
    ) -> Option<HExpr> {
        use dream_syntax::nodes::types::mangle_generic;
        use dream_types::constructor_fn;
        let mut throwaway = dream_diagnostics::DiagnosticBag::new(None);
        let no_span = TextSpan {
            start: 0,
            end: 0,
            line_no: 0,
            col_no: 0,
        };
        self.ensure_struct_instantiated(
            base,
            std::slice::from_ref(elem_ty),
            &no_span,
            &mut throwaway,
        );
        let mangled = mangle_generic(base, std::slice::from_ref(elem_ty));
        let def = self.type_ctx.resolve(DefKind::Struct, base)?;
        let ctor = self
            .type_ctx
            .resolve(DefKind::Function, &constructor_fn(&mangled));
        let ty = self.type_ctx.lower(&boxed_ty);
        Some(HExpr::new(
            ty,
            HExprKind::New {
                def,
                instance: vec![],
                ctor,
                args: vec![value],
                take_params: self.ctor_take_params(ctor),
            },
        ))
    }

    /// Records the HIR for a resolved call to a generic free function. `base_name` is the template's
    /// (unmangled) name — the `DefId` shared by every instance — and `instance` is the concrete
    /// type-args (in binding order) that select the monomorphization. The backend combines
    /// `(def, instance)` into the same symbol the instance body emits. Drops out of coverage if the
    /// base name is unregistered or any argument is not representable.
    pub(in crate::analyzer) fn hir_set_generic_call(
        &mut self,
        base_name: &str,
        instance: Vec<TypeId>,
        args: Vec<Option<HExpr>>,
        ret: &Type,
        take_params: Vec<bool>,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let Some(def) = self.type_ctx.resolve(DefKind::Function, base_name) else {
            self.hir.last = None;
            return;
        };
        let Some(collected) = Self::collect_hir_args(args) else {
            self.hir.last = None;
            return;
        };
        let ret_ty = self.type_ctx.lower(ret);
        let callee = Callee {
            def,
            instance,
            ret: ret_ty,
            take_params,
        };
        self.hir.last = Some(HExpr::new(
            ret_ty,
            HExprKind::Call {
                callee,
                args: collected,
            },
        ));
    }
}
