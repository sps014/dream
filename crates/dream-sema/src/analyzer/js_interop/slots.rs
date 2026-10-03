use super::*;

impl<'a> Analyzer<'a> {
    /// A `string` literal HExpr (for the dynamic member/method name).
    pub(in crate::analyzer) fn js_name_lit(&self, name: &str) -> HExpr {
        let string = self.type_ctx.interner.string();
        HExpr::new(string, HExprKind::StringLit(name.to_string()))
    }

    /// Prepares one argument for a shadow-stack `js` call *slot*: unlike [`box_to_js`], primitives
    /// are NOT boxed into handles (the host reads them straight out of the tagged slot); only a
    /// `float` is widened to `double` so its slot payload is an `f64`. `js`, `string`, primitive,
    /// `enum`, a `fun(js)`/`fun()` callback, and a primitive/`string`/`js` array are all accepted as
    /// they are; any other type returns `None` (a compile error pointing at `js.object()`/`js.array()`).
    ///
    /// Capturing callbacks are rejected by the caller ([`js_slot_args`]) before this runs.
    pub(in crate::analyzer) fn js_slot_arg(&mut self, e: HExpr) -> Option<HExpr> {
        let stripped = e.ty;
        let kind = self.type_ctx.interner.kind(stripped).clone();
        match kind {
            TyKind::Js | TyKind::Enum(_) => Some(e),
            TyKind::Prim(PrimTy::Float) => Some(self.cast_prim(e, PrimTy::Double)),
            TyKind::Prim(_) => Some(e),
            // A callback slot carries its arity in the slot `aux` word (see `js_abi::slot_desc`), so
            // the host wraps the funcref as `fun(js, …): void` with the right number of `js`
            // parameters. Any arity is marshalable through the slot buffer (env is stripped at emit).
            TyKind::Func(..) => Some(e),
            TyKind::Array(elem) => {
                let ek = self.type_ctx.interner.kind(elem).clone();
                match ek {
                    TyKind::Prim(_) | TyKind::Js | TyKind::Enum(_) => Some(e),
                    _ => None,
                }
            }
            // A struct/class argument deep-copies into a JS object handle (a JS slot).
            TyKind::Struct(..) => self.box_to_js(e, None, None),
            _ => None,
        }
    }

    /// Prepares every argument via [`js_slot_arg`], reporting a compile error and returning `None` on
    /// the first non-marshalable one.
    pub(in crate::analyzer) fn js_slot_args(
        &mut self,
        args: Vec<Option<HExpr>>,
        pos: Option<TextSpan>,
        diagnostics: &mut DiagnosticBag,
    ) -> Option<Vec<HExpr>> {
        let mut out = Vec::with_capacity(args.len());
        for arg in args {
            let arg = arg?;
            if matches!(self.type_ctx.interner.kind(arg.ty), TyKind::Func(..))
                && !self.ensure_captureless_js_callback(&arg, pos, diagnostics)
            {
                return None;
            }
            let arg_display =
                dream_types::display_name(&self.type_ctx.interner, &self.type_ctx.defs, arg.ty);
            match self.js_slot_arg(arg) {
                Some(a) => out.push(a),
                None => {
                    diagnostics.report_error(
                        format!(
                            "cannot pass a value of type '{}' to a js call; build a JS value with js.object() / js.array() and set its members natively",
                            arg_display
                        ),
                        pos,
                    );
                    return None;
                }
            }
        }
        Some(out)
    }

    /// Builds a `JsCall` HIR node targeting a shadow-stack `js` bridge (`call` / `invoke` /
    /// `get_call` / `*_as_*` / `set_slot` / `index_set_slot`). `via` is the property to read
    /// before calling when fusing get+call. Returns `None` only if the bridge is somehow
    /// unregistered (a stdlib bug).
    pub(in crate::analyzer) fn js_call_node(
        &self,
        bridge: &str,
        target: HExpr,
        via: Option<HExpr>,
        method: Option<HExpr>,
        args: Vec<HExpr>,
        ret: TypeId,
    ) -> Option<HExpr> {
        let mangled = method_fn(dream_abi::js_abi::JS_TYPE, bridge);
        let def = self.type_ctx.defs.lookup(DefKind::Function, &mangled)?;
        Some(HExpr::new(
            ret,
            HExprKind::JsCall {
                callee: Callee {
                    def,
                    instance: vec![],
                    ret,
                    take_params: vec![],
                },
                target: Box::new(target),
                via: via.map(Box::new),
                method: method.map(Box::new),
                args,
            },
        ))
    }

    /// If `recv` is a pure `js.get(base, prop)` (possibly under a cast), peel it into
    /// `(base, Some(prop))` for fused `get_call`; otherwise `(recv, None)`.
    pub(in crate::analyzer) fn peel_js_get_recv(&self, recv: HExpr) -> (HExpr, Option<HExpr>) {
        if let Some((base, prop)) = self.match_js_get(&recv) {
            (base, Some(prop))
        } else {
            (recv, None)
        }
    }

    /// `recv.name` -> `js.get(recv, "name")`. Sets the last-expression HIR.
    pub(in crate::analyzer) fn desugar_js_get(&mut self, recv: Option<HExpr>, name: &str) {
        if !self.hir_active() {
            self.hir_none();
            return;
        }
        let js = self.type_ctx.interner.js();
        let name_lit = self.js_name_lit(name);
        let call = match recv {
            Some(recv) => self.js_bridge_call("get", vec![recv, name_lit], js),
            None => None,
        };
        self.hir_set_last(call);
    }

    /// True when `ty` can ride a shadow-stack slot for `set_slot` / `index_set_slot` (same set as
    /// call args, minus callbacks/arrays which still need the handle `set` path for property
    /// identity).
    pub(in crate::analyzer) fn js_slot_settable(&self, ty: TypeId) -> bool {
        matches!(
            self.type_ctx.interner.kind(ty),
            TyKind::Js | TyKind::Enum(_) | TyKind::Prim(_)
        )
    }

    /// `recv.name = value` -> `js.set_slot` for slot-marshalable values (one crossing, no pre-box),
    /// else `js.set(recv, "name", box(value))`. Emits a void statement.
    pub(in crate::analyzer) fn desugar_js_set(
        &mut self,
        recv: Option<HExpr>,
        name: &str,
        value: Option<HExpr>,
        pos: Option<TextSpan>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if !self.hir_active() {
            return;
        }
        let void = self.type_ctx.interner.void();
        let name_lit = self.js_name_lit(name);
        let (Some(recv), Some(value)) = (recv, value) else {
            self.hir_fail();
            return;
        };
        if self.js_slot_settable(value.ty) {
            let Some(value) = self.js_slot_arg(value) else {
                self.hir_fail();
                return;
            };
            let call = self.js_call_node("set_slot", recv, None, Some(name_lit), vec![value], void);
            self.hir_expr_stmt(call);
            return;
        }
        let Some(value) = self.box_to_js(value, pos, Some(diagnostics)) else {
            // `box_to_js` already reported the capturing-callback diagnostic when applicable.
            if !diagnostics.has_errors() {
                diagnostics.report_error(
                    "cannot assign this value to a js property; build a JS value with js.object() / js.array()".to_string(),
                    pos,
                );
            }
            self.hir_fail();
            return;
        };
        let call = self.js_bridge_call("set", vec![recv, name_lit, value], void);
        self.hir_expr_stmt(call);
    }

    /// `recv[key]` -> `js.index_get(recv, box(key))`. Sets `hir.last`.
    pub(in crate::analyzer) fn desugar_js_index_get(
        &mut self,
        recv: Option<HExpr>,
        key: Option<HExpr>,
        pos: Option<TextSpan>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if !self.hir_active() {
            self.hir_none();
            return;
        }
        let js = self.type_ctx.interner.js();
        let (Some(recv), Some(key)) = (recv, key) else {
            self.hir_none();
            return;
        };
        let Some(key) = self.box_to_js(key, pos, Some(diagnostics)) else {
            if !diagnostics.has_errors() {
                diagnostics
                    .report_error("cannot use this value as a js index key".to_string(), pos);
            }
            self.hir_none();
            return;
        };
        let call = self.js_bridge_call("index_get", vec![recv, key], js);
        self.hir_set_last(call);
    }

    /// `recv[key] = value` -> `js.index_set_slot` when both are slot-marshalable, else boxed
    /// `js.index_set`. Emits a void statement.
    pub(in crate::analyzer) fn desugar_js_index_set(
        &mut self,
        recv: Option<HExpr>,
        key: Option<HExpr>,
        value: Option<HExpr>,
        pos: Option<TextSpan>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if !self.hir_active() {
            return;
        }
        let void = self.type_ctx.interner.void();
        let (Some(recv), Some(key), Some(value)) = (recv, key, value) else {
            self.hir_fail();
            return;
        };
        if self.js_slot_settable(key.ty) && self.js_slot_settable(value.ty) {
            let (Some(key), Some(value)) = (self.js_slot_arg(key), self.js_slot_arg(value)) else {
                self.hir_fail();
                return;
            };
            let call =
                self.js_call_node("index_set_slot", recv, None, None, vec![key, value], void);
            self.hir_expr_stmt(call);
            return;
        }
        let key = self.box_to_js(key, pos, Some(diagnostics));
        let value = self.box_to_js(value, pos, Some(diagnostics));
        let (Some(key), Some(value)) = (key, value) else {
            if !diagnostics.has_errors() {
                diagnostics.report_error(
                    "cannot use this value as a js index key/value".to_string(),
                    pos,
                );
            }
            self.hir_fail();
            return;
        };
        let call = self.js_bridge_call("index_set", vec![recv, key, value], void);
        self.hir_expr_stmt(call);
    }
}
