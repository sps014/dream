//! Desugaring of native syntax on the dynamic `js` type into calls to the stdlib interop bridges
//! declared in `stdlib/core/js.dream`.
//!
//! When a receiver has type `js`, member access, method calls, indexing, property assignment, and
//! calling the value itself all bind *dynamically*: the compiler performs no member resolution and
//! instead lowers the operation to a fixed bridge extern. Variadic call/invoke (and slot set)
//! arguments are written into a shadow-stack buffer of tagged 16-byte slots so every argument
//! crosses in a single host call.
//!
//! Hot paths are auto-specialized when the shape is known at HIR emit time:
//! - a property get or call immediately coerced to a primitive/`string` becomes a fused `*_as_*`
//!   bridge (no intermediate handle);
//! - a pure `get` used only as the receiver of a call becomes `get_call` (one crossing);
//! - property/index writes of slot-marshalable values use `set_slot` / `index_set_slot` (no
//!   pre-box bridge).
//!
//! Every dynamic operation that stays in the `js` world still yields `js`; conversions back to
//! Dream values happen at typed boundaries (see the box/unbox helpers, also used by `coerce_to`)
//! or via the explicit `js.to_int()` etc.

use super::synthetic_token;
use crate::analyzer::Analyzer;
use crate::errors::SemanticError;
use dream_diagnostics::DiagnosticBag;
use dream_hir::{Binding, Callee, HExpr, HExprKind};
use dream_syntax::nodes::{ExpressionNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;
use dream_text::text_span::TextSpan;
use dream_types::{method_fn, DefId, DefKind, PrimTy, TyKind, TypeId};

mod conversions;
mod slots;
impl<'a> Analyzer<'a> {
    /// The legacy AST `Type` for the dynamic `js` type (a bare nominal name the type context lowers
    /// to `TyKind::Js`).
    pub(super) fn js_type() -> Type {
        Type::Struct(
            synthetic_token(TokenKind::IdentifierToken, dream_abi::js_abi::JS_TYPE),
            None,
        )
    }

    /// True if `ty` is the dynamic `js` type. `js` is represented at the AST layer as a nominal type
    /// whose spelling is exactly [`js_abi::JS_TYPE`](dream_abi::js_abi::JS_TYPE); comparing against
    /// that shared constant (rather than a bare literal) keeps recognition in lockstep with the
    /// bridge-mangling side, and the exact match excludes `js[]` / `js?`.
    pub(super) fn is_js_type(&self, ty: &Type) -> bool {
        ty.get_type() == dream_abi::js_abi::JS_TYPE
    }

    /// Diagnostic when a capturing `fun(...)` value is handed to a JS API. The host bridges
    /// (`func0`/`func`/`funcN`, FUNC slots) only take the funcidx half of a funcbox — the env
    /// word is discarded — so a capturing lambda would lose its environment.
    const JS_CAPTURING_CALLBACK_MSG: &'static str = "capturing lambdas cannot be passed to JS APIs (the closure environment would be lost); pass a non-capturing top-level function, or wrap only a captureless `fun(...)` via `js.func` / `js.func0`";

    /// True when `e` is a known-capturing `fun(...)` value: a `funcbox_new` with a non-zero env,
    /// a `Binding::Func` whose def is a capturing lambda/method-group, or a fun-typed local marked
    /// capturing in [`Self::capturing_fun_locals`].
    pub(in crate::analyzer) fn func_expr_is_capturing(&self, e: &HExpr) -> bool {
        match &e.kind {
            HExprKind::Cast(inner) => self.func_expr_is_capturing(inner),
            HExprKind::Call { callee, args } => {
                if self.closure_intrinsic("funcbox_new") == Some(callee.def) && args.len() >= 2 {
                    let env_nonzero = !matches!(args[1].kind, HExprKind::IntLit(0));
                    if env_nonzero {
                        return true;
                    }
                    return self.func_raw_is_capturing_def(&args[0]);
                }
                false
            }
            HExprKind::Var(Binding::Func(c)) => self.def_is_capturing_fun(c.def),
            HExprKind::Var(Binding::Local(id)) => self
                .hir_local_name(*id)
                .and_then(|n| self.capturing_fun_locals.get(n).copied())
                .unwrap_or(false),
            _ => false,
        }
    }

    fn func_raw_is_capturing_def(&self, e: &HExpr) -> bool {
        match &e.kind {
            HExprKind::Cast(inner) => self.func_raw_is_capturing_def(inner),
            HExprKind::Var(Binding::Func(c)) => self.def_is_capturing_fun(c.def),
            _ => false,
        }
    }

    fn def_is_capturing_fun(&self, def: DefId) -> bool {
        let name = self.type_ctx.defs.name(def);
        self.closure_captures
            .get(name)
            .is_some_and(|caps| !caps.is_empty())
    }

    /// Records whether a fun-typed local's current value is capturing, for later JS-boundary checks.
    pub(in crate::analyzer) fn record_capturing_fun_local(
        &mut self,
        name: &str,
        ty: &Type,
        value: Option<&HExpr>,
    ) {
        if !matches!(ty, Type::Function(_, _)) {
            return;
        }
        let capturing = value.is_some_and(|v| self.func_expr_is_capturing(v));
        self.capturing_fun_locals
            .insert(name.to_string(), capturing);
    }

    /// Reports [`Self::JS_CAPTURING_CALLBACK_MSG`] and returns `false` when `e` is a capturing
    /// callback; otherwise returns `true`.
    pub(in crate::analyzer) fn ensure_captureless_js_callback(
        &self,
        e: &HExpr,
        pos: Option<TextSpan>,
        diagnostics: &mut DiagnosticBag,
    ) -> bool {
        if self.func_expr_is_capturing(e) {
            diagnostics.report_error(Self::JS_CAPTURING_CALLBACK_MSG.to_string(), pos);
            false
        } else {
            true
        }
    }

    /// Builds a call to a `js` bridge extern (`js.__something`), resolved by its mangled def name.
    /// Returns `None` only if the bridge is somehow unregistered (a stdlib bug).
    fn js_bridge_call(&self, method: &str, args: Vec<HExpr>, ret: TypeId) -> Option<HExpr> {
        let mangled = method_fn(dream_abi::js_abi::JS_TYPE, method);
        let def = self.type_ctx.resolve(DefKind::Function, &mangled)?;
        Some(HExpr::new(
            ret,
            HExprKind::Call {
                callee: Callee {
                    def,
                    instance: vec![],
                    ret,
                    take_params: vec![],
                },
                args,
            },
        ))
    }

    /// Analyzes a method call `recv.method(args)` on a `js` receiver. A method actually declared on
    /// `js` (the stdlib conversion/release helpers such as `to_int`, `is_null`, `release`) is
    /// dispatched normally; any other name binds dynamically at runtime via `call`.
    pub(super) fn analyze_js_member_call(
        &mut self,
        recv: Option<HExpr>,
        method: &SyntaxToken,
        params: &Vec<ExpressionNode<'a>>,
        ctx: &super::AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let mangled = method_fn(dream_abi::js_abi::JS_TYPE, &method.text);
        // Cloned up front (rather than re-looked-up below) because the argument analysis loop needs
        // `&mut self`, which would otherwise conflict with a borrow held from this lookup.
        let known_sig = self.function_table.get_function(&mangled).ok();

        let mut arg_hirs = Vec::with_capacity(params.len());
        for (i, param) in params.iter().enumerate() {
            let saved_expected = self.current_expected_type.take();
            if let Some(ref sig) = known_sig {
                self.current_expected_type = sig.parameter_types.get(i).cloned();
            }
            let _ =
                self.analyze_expression(param, ctx.parent_function, ctx.symbol_table, diagnostics)?;
            self.current_expected_type = saved_expected;
            arg_hirs.push(self.hir_take());
        }

        if let Some(sig) = known_sig {
            // Explicit `js.func` / `js.func0` / `js.funcN` also strip the env word host-side —
            // reject capturing handlers here (they skip `box_to_js` / FUNC-slot checks).
            if matches!(method.text.as_str(), "func" | "func0" | "funcN") {
                for arg in arg_hirs.iter().flatten() {
                    if matches!(self.type_ctx.interner.kind(arg.ty), TyKind::Func(..))
                        && !self.ensure_captureless_js_callback(
                            arg,
                            Some(method.position),
                            diagnostics,
                        )
                    {
                        self.hir_none();
                        return Ok(Type::Unknown);
                    }
                }
            }
            let ret = sig.return_type.clone().unwrap_or(Type::Void);
            self.hir_set_method_call(recv, &sig.name, arg_hirs, &ret);
            return Ok(ret);
        }

        self.desugar_js_call(
            recv,
            &method.text,
            arg_hirs,
            Some(method.position),
            diagnostics,
        );
        Ok(Self::js_type())
    }

    /// `recv.name(args...)` -> `js.call` or fused `js.get_call` when `recv` is a pure get.
    /// Sets `hir.last`.
    pub(super) fn desugar_js_call(
        &mut self,
        recv: Option<HExpr>,
        name: &str,
        args: Vec<Option<HExpr>>,
        pos: Option<TextSpan>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if !self.hir_active() {
            self.hir_none();
            return;
        }
        let name_lit = self.js_name_lit(name);
        let Some(recv) = recv else {
            self.hir_none();
            return;
        };
        let Some(args) = self.js_slot_args(args, pos, diagnostics) else {
            self.hir_none();
            return;
        };
        let js = self.type_ctx.interner.js();
        let (target, via) = self.peel_js_get_recv(recv);
        let bridge = if via.is_some() { "get_call" } else { "call" };
        let call = self.js_call_node(bridge, target, via, Some(name_lit), args, js);
        self.hir_set_last(call);
    }

    /// `recv(args...)` -> `js.invoke(recv, slots…)`. Sets `hir.last`.
    pub(super) fn desugar_js_invoke(
        &mut self,
        recv: Option<HExpr>,
        args: Vec<Option<HExpr>>,
        pos: Option<TextSpan>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if !self.hir_active() {
            self.hir_none();
            return;
        }
        let Some(recv) = recv else {
            self.hir_none();
            return;
        };
        let Some(args) = self.js_slot_args(args, pos, diagnostics) else {
            self.hir_none();
            return;
        };
        let js = self.type_ctx.interner.js();
        let call = self.js_call_node("invoke", recv, None, None, args, js);
        self.hir_set_last(call);
    }

    /// `js.global` (the bare property, not the `js.global("name")` call) -> `globalThis`, so member
    /// access chains like `js.global.document` / `js.global.fetch(...)` bind against the JS global
    /// scope. Sets `hir.last`.
    pub(super) fn desugar_js_global_this(&mut self) {
        if !self.hir_active() {
            self.hir_none();
            return;
        }
        let js = self.type_ctx.interner.js();
        let call = self.js_bridge_call("global_this", vec![], js);
        self.hir_set_last(call);
    }

    /// The AST type `Option<js>` (the result of awaiting a `js` Promise).
    pub(super) fn option_js_type() -> Type {
        Type::Struct(
            synthetic_token(TokenKind::IdentifierToken, "Option"),
            Some(vec![Self::js_type()]),
        )
    }

    /// `<jsExpr>.await` -> `js.await_promise(<jsExpr>).await`. Builds the async wrapper call whose result is
    /// `Future<Option<js>>` (so the enclosing `.await` unwraps it to `Option<js>` - `Some` on resolve,
    /// `None` on rejection), letting a JS Promise be awaited natively. Returns the
    /// `Future<Option<js>>`-typed call HIR (to hand to `hir_set_await`), or `None` if the inner
    /// expression was not representable.
    pub(super) fn desugar_js_await(&mut self, inner: Option<HExpr>) -> Option<HExpr> {
        let recv = inner?;
        let fut = self
            .type_ctx
            .lower(&Self::future_type(Self::option_js_type()));
        // `await_promise`'s `token` parameter defaults to `Option.None`, but default substitution
        // only runs for calls written in source — a synthetic bridge call has to spell it out.
        let token = self.option_none(&Self::cancellation_token_type())?;
        self.js_bridge_call("await_promise", vec![recv, token], fut)
    }

    /// The AST type of `CancellationToken`.
    fn cancellation_token_type() -> Type {
        Type::Struct(
            synthetic_token(TokenKind::IdentifierToken, "CancellationToken"),
            None,
        )
    }

    /// `Option<inner>.None` as HIR, instantiating `Option<inner>` if this is its first use.
    fn option_none(&mut self, inner: &Type) -> Option<HExpr> {
        use dream_syntax::nodes::types::mangle_generic;
        let mut throwaway = DiagnosticBag::new(None);
        let no_span = TextSpan {
            start: 0,
            end: 0,
            line_no: 0,
            col_no: 0,
        };
        let args = std::slice::from_ref(inner);
        self.ensure_union_instantiated("Option", args, &no_span, &mut throwaway);
        let mangled = mangle_generic("Option", args);
        let def = self.type_ctx.resolve(DefKind::Union, &mangled)?;
        let variant = self.union_info(&mangled)?.variant("None")?.discriminant as usize;
        let ty = self.type_ctx.lower(&Type::Struct(
            synthetic_token(TokenKind::IdentifierToken, "Option"),
            Some(vec![inner.clone()]),
        ));
        Some(HExpr::new(
            ty,
            HExprKind::UnionNew {
                def,
                variant,
                args: vec![],
            },
        ))
    }
}
