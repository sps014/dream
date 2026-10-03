use super::*;

impl<'a> Analyzer<'a> {
    /// Wraps `e` in an implicit cast to primitive `prim` (for widening a boxing argument to the
    /// bridge's declared parameter type, e.g. `float` -> `double`).
    pub(in crate::analyzer) fn cast_prim(&mut self, e: HExpr, prim: PrimTy) -> HExpr {
        let ty = self.type_ctx.interner.prim(prim);
        HExpr::new(ty, HExprKind::Cast(Box::new(e)))
    }

    /// Boxes a Dream value into a `js` handle: a `js` value passes through; primitives/`string` route
    /// through the matching `__box_*` bridge; a `fun(js): void` / `fun(): void` is wrapped as a JS
    /// callable. Any other type (struct/class/union/array/list) yields `None` (a compile error at the
    /// call site, pointing at `js.object()` / `js.array()`).
    ///
    /// A capturing `fun(...)` yields `None` after reporting via `diagnostics` when provided — the
    /// host bridges strip the closure env word, so only captureless functions are marshalable.
    pub(in crate::analyzer) fn box_to_js(
        &mut self,
        e: HExpr,
        pos: Option<TextSpan>,
        diagnostics: Option<&mut DiagnosticBag>,
    ) -> Option<HExpr> {
        let js = self.type_ctx.interner.js();
        let stripped = e.ty;
        let kind = self.type_ctx.interner.kind(stripped).clone();
        match kind {
            TyKind::Js => Some(e),
            TyKind::Enum(_) => self.js_bridge_call("box_int", vec![e], js),
            TyKind::Prim(p) => match p {
                PrimTy::String => self.js_bridge_call("box_string", vec![e], js),
                PrimTy::Bool => self.js_bridge_call("box_bool", vec![e], js),
                PrimTy::Double => self.js_bridge_call("box_double", vec![e], js),
                PrimTy::Float => {
                    let d = self.cast_prim(e, PrimTy::Double);
                    self.js_bridge_call("box_double", vec![d], js)
                }
                PrimTy::Long | PrimTy::ULong | PrimTy::ISize | PrimTy::USize => {
                    let wide =
                        HExpr::new(self.type_ctx.interner.long(), HExprKind::Cast(Box::new(e)));
                    self.js_bridge_call("box_long", vec![wide], js)
                }
                PrimTy::Int => self.js_bridge_call("box_int", vec![e], js),
                PrimTy::UInt | PrimTy::Byte | PrimTy::Char => {
                    let i = self.cast_prim(e, PrimTy::Int);
                    self.js_bridge_call("box_int", vec![i], js)
                }
            },
            TyKind::Func(params, _ret) => {
                // A Dream function handed to a JS API as a persistent handle. `e` is a boxed
                // `fun(...)` value (see `hir_set_func_value`); the host has no env-restoring
                // prologue of its own, so only the funcidx half is meaningful — a *capturing*
                // lambda would lose its environment and is rejected at compile time. Arity 0/1 use
                // the documented `func0`/`func` convenience bridges; any higher arity routes through
                // the generalized `funcN` bridge, which receives the raw funcref-table index plus
                // the parameter count and wraps it host-side as `fun(js, …): void`. Each parameter
                // is marshaled as a `js` handle and the result is discarded.
                if self.func_expr_is_capturing(&e) {
                    if let Some(diagnostics) = diagnostics {
                        diagnostics.report_error(Self::JS_CAPTURING_CALLBACK_MSG.to_string(), pos);
                    }
                    return None;
                }
                let funcidx = self.hir_funcbox_funcidx(e)?;
                match params.len() {
                    0 => self.js_bridge_call("func0", vec![funcidx], js),
                    1 => self.js_bridge_call("func", vec![funcidx], js),
                    n => {
                        let arity =
                            HExpr::new(self.type_ctx.interner.int(), HExprKind::IntLit(n as i64));
                        self.js_bridge_call("funcN", vec![funcidx, arity], js)
                    }
                }
            }
            // A struct/class deep-copies into a plain JS object; the backend generates a
            // `$<Type>_to_js` marshaler that the `Cast` dispatches to (see `mir/emit/js_marshal.rs`).
            TyKind::Struct(..) => Some(HExpr::new(js, HExprKind::Cast(Box::new(e)))),
            _ => None,
        }
    }

    /// Unboxes a `js` value into primitive/`string` `target`, via the matching `__as_*` bridge (plus
    /// a widening/narrowing cast when `target` is not the bridge's own result type). Used at typed
    /// boundaries by `coerce_to`. When `e` is a fresh `js.get` / `JsCall`, rewrites to a fused
    /// `get_as_*` / `call_as_*` / `get_call_as_*` bridge so the intermediate handle is never
    /// registered.
    pub(in crate::analyzer) fn unbox_from_js(&mut self, e: HExpr, target: TypeId) -> HExpr {
        let target_stripped = target;
        // A struct/class target reconstructs from the JS object's properties via the generated
        // `$js_to_<Type>` marshaler that the `Cast` dispatches to (heap result for reference
        // classes; in-place `(j, dst)` fill for value structs — see `mir/emit/js_marshal.rs`).
        if matches!(
            self.type_ctx.interner.kind(target_stripped),
            TyKind::Struct(..)
        ) {
            return HExpr::new(target_stripped, HExprKind::Cast(Box::new(e)));
        }
        let TyKind::Prim(p) = self.type_ctx.interner.kind(target_stripped).clone() else {
            return e;
        };
        let Some(suffix) = Self::js_as_suffix(p) else {
            return e;
        };
        let bridge_ret = self.js_as_bridge_ret(p);
        if let Some(fused) = self.try_fuse_unbox(e.clone(), suffix, bridge_ret) {
            return self.js_widen_as_result(fused, p, target_stripped);
        }
        let call = self.js_bridge_call(&format!("as_{}", suffix), vec![e], bridge_ret);
        let raw = call.unwrap_or_else(|| HExpr::new(bridge_ret, HExprKind::IntLit(0)));
        self.js_widen_as_result(raw, p, target_stripped)
    }

    /// Suffix of the `as_*` / `get_as_*` / `call_as_*` bridge for `p` (`"int"`, `"string"`, …).
    pub(in crate::analyzer) fn js_as_suffix(p: PrimTy) -> Option<&'static str> {
        match p {
            PrimTy::String => Some("string"),
            PrimTy::Bool => Some("bool"),
            PrimTy::Double | PrimTy::Float => Some("double"),
            PrimTy::Long | PrimTy::ULong | PrimTy::ISize | PrimTy::USize => Some("long"),
            PrimTy::Int | PrimTy::UInt | PrimTy::Byte | PrimTy::Char => Some("int"),
        }
    }

    /// Return type of the matching `as_*` bridge (before any widening cast to the Dream target).
    pub(in crate::analyzer) fn js_as_bridge_ret(&self, p: PrimTy) -> TypeId {
        match p {
            PrimTy::String => self.type_ctx.interner.string(),
            PrimTy::Bool => self.type_ctx.interner.bool(),
            PrimTy::Double | PrimTy::Float => self.type_ctx.interner.double(),
            PrimTy::Long | PrimTy::ULong | PrimTy::ISize | PrimTy::USize => {
                self.type_ctx.interner.long()
            }
            PrimTy::Int | PrimTy::UInt | PrimTy::Byte | PrimTy::Char => {
                self.type_ctx.interner.int()
            }
        }
    }

    /// Casts a fused/`as_*` bridge result to `target` when the Dream binding type is narrower or a
    /// different integer width than the bridge's native return (e.g. `float` from `as_double`,
    /// `byte` from `as_int`).
    pub(in crate::analyzer) fn js_widen_as_result(
        &self,
        raw: HExpr,
        p: PrimTy,
        target: TypeId,
    ) -> HExpr {
        match p {
            PrimTy::Float | PrimTy::UInt | PrimTy::Byte | PrimTy::Char | PrimTy::ULong => {
                HExpr::new(target, HExprKind::Cast(Box::new(raw)))
            }
            _ => {
                if raw.ty == target {
                    raw
                } else {
                    HExpr::new(target, HExprKind::Cast(Box::new(raw)))
                }
            }
        }
    }

    /// True when `def` is the mangled `js.<method>` bridge.
    pub(in crate::analyzer) fn is_js_bridge_def(&self, def: DefId, method: &str) -> bool {
        self.type_ctx.defs.name(def) == method_fn(dream_abi::js_abi::JS_TYPE, method)
    }

    /// Peels a trivial `Cast` wrapper so fusion can see the underlying bridge call.
    pub(in crate::analyzer) fn peel_js_cast(e: HExpr) -> HExpr {
        match e.kind {
            HExprKind::Cast(inner) => Self::peel_js_cast(*inner),
            _ => e,
        }
    }

    /// If `e` is `js.get(recv, name)`, returns `(recv, name)`.
    pub(in crate::analyzer) fn match_js_get(&self, e: &HExpr) -> Option<(HExpr, HExpr)> {
        let e = match &e.kind {
            HExprKind::Cast(inner) => inner.as_ref(),
            _ => e,
        };
        match &e.kind {
            HExprKind::Call { callee, args }
                if args.len() == 2 && self.is_js_bridge_def(callee.def, "get") =>
            {
                Some((args[0].clone(), args[1].clone()))
            }
            _ => None,
        }
    }

    /// Rewrites a fresh get / call / get_call into the matching `*_as_*` bridge, or `None` when
    /// `e` is not a fusible dynamic op (stored intermediate, unknown shape, …).
    pub(in crate::analyzer) fn try_fuse_unbox(
        &self,
        e: HExpr,
        suffix: &str,
        ret: TypeId,
    ) -> Option<HExpr> {
        let e = Self::peel_js_cast(e);
        if let Some((recv, name)) = self.match_js_get(&e) {
            return self.js_bridge_call(&format!("get_as_{}", suffix), vec![recv, name], ret);
        }
        match e.kind {
            HExprKind::JsCall {
                callee: _,
                target,
                via,
                method,
                args,
            } => {
                let bridge = match (&via, &method) {
                    (Some(_), Some(_)) => format!("get_call_as_{}", suffix),
                    (None, Some(_)) => format!("call_as_{}", suffix),
                    (None, None) => format!("invoke_as_{}", suffix),
                    (Some(_), None) => return None,
                };
                self.js_call_node(
                    &bridge,
                    *target,
                    via.map(|v| *v),
                    method.map(|m| *m),
                    args,
                    ret,
                )
            }
            _ => None,
        }
    }
}
