use super::*;

impl<'a> Analyzer<'a> {
    /// Reads the `$__closure_env` module global (an `int`): the environment word a caller sets
    /// (see `hir_set_indirect_call_expr`) just before an indirect call through a boxed closure
    /// value, which the callee's own prologue reads here to recover its captured cells (see
    /// `Analyzer::hir_begin_function`'s capturing-lambda prologue).
    pub(in crate::analyzer) fn hir_read_closure_env(&mut self) -> Option<HExpr> {
        let &(env_global, ty) = self.hir.globals.get("__closure_env")?;
        Some(HExpr::new(ty, HExprKind::Var(Binding::Global(env_global))))
    }

    /// Looks up one of the `Closure.*` compiler-internal intrinsics (see
    /// `src/stdlib/core/closure.dream`) by its bare method name. These back the `fun(...)` closure
    /// ABI (a boxed `[funcidx][env]` heap value); every call site is built directly here rather than
    /// through ordinary name resolution, so the stdlib class is never referenced by user code.
    pub(in crate::analyzer) fn closure_intrinsic(&self, method: &str) -> Option<DefId> {
        let owner = self.type_ctx.resolved_type("Closure")?;
        self.unique_method_def(owner, method)
    }

    /// Wraps a raw function-table index (`raw`, always `int`-typed) plus an optional environment
    /// pointer (`env`, `None` for a non-capturing value — becomes a `0` literal) into a boxed
    /// `fun(...)` value via `Closure.funcbox_new`, typed as `func_ty` (the box is a plain `i32` at
    /// runtime regardless of the declared `fun(...)` shape it carries). Returns `None` (dropping HIR
    /// coverage) if the `Closure` intrinsics are not registered — should not happen, since the
    /// stdlib prelude always defines them, but this keeps the failure a silent coverage drop rather
    /// than a panic if the prelude is ever missing them.
    pub(in crate::analyzer) fn build_funcbox(
        &mut self,
        raw: HExpr,
        env: Option<HExpr>,
        func_ty: &Type,
    ) -> Option<HExpr> {
        let new_def = self.closure_intrinsic("funcbox_new")?;
        let int_ty = self.type_ctx.interner.int();
        let env_expr = env.unwrap_or_else(|| HExpr::new(int_ty, HExprKind::IntLit(0)));
        let box_ty = self.type_ctx.lower(func_ty);
        let callee = Callee {
            def: new_def,
            instance: vec![],
            ret: box_ty,
            take_params: vec![],
        };
        Some(HExpr::new(
            box_ty,
            HExprKind::Call {
                callee,
                args: vec![raw, env_expr],
            },
        ))
    }

    /// Extracts a boxed `fun(...)` value's raw function-table index, discarding its environment word
    /// — used at boundaries with no env-restoring prologue of their own (a host `@js` bridge; see
    /// `js_interop::box_to_js`), where only the funcidx half of the box is meaningful. `None` if the
    /// `Closure` intrinsics are unavailable.
    pub(in crate::analyzer) fn hir_funcbox_funcidx(&mut self, boxed: HExpr) -> Option<HExpr> {
        let def = self.closure_intrinsic("funcbox_funcidx")?;
        let int_ty = self.type_ctx.interner.int();
        Some(HExpr::new(
            int_ty,
            HExprKind::Call {
                callee: Callee {
                    def,
                    instance: vec![],
                    ret: int_ty,
                    take_params: vec![],
                },
                args: vec![boxed],
            },
        ))
    }

    /// Records a first-class function value: a bare function name used as a value (e.g. `let f = foo;`
    /// or passing `foo` to a `fun(...)` parameter) resolves its `Binding::Func` (the def + signature)
    /// to a raw function-table index, then boxes it (with a null environment — see [`build_funcbox`])
    /// so it carries the same runtime shape as a capturing closure. Drops coverage if the name is not
    /// a registered function def.
    pub(in crate::analyzer) fn hir_set_func_value(
        &mut self,
        name: &str,
        func_ty: &Type,
        ret: &Type,
    ) {
        let Ok(info) = self.function_info(name) else { self.hir.last = None; return; };
        self.hir_set_func_value_identity(&info.identity, func_ty, ret);
    }

    pub(in crate::analyzer) fn hir_set_func_value_identity(
        &mut self,
        identity: &crate::function_table::FunctionIdentity,
        func_ty: &Type,
        ret: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let int_ty = self.type_ctx.interner.int();
        let ret_ty = self.type_ctx.lower(ret);
        let raw = HExpr::new(
            int_ty,
            HExprKind::Var(Binding::Func(Callee {
                def: identity.0,
                instance: identity.1.clone(),
                ret: ret_ty,
                take_params: vec![],
            })),
        );
        self.hir.last = self.build_funcbox(raw, None, func_ty);
    }

    /// Like [`hir_set_func_value`], but wraps the box around a *captured* environment (a
    /// `CaptureCell<T>` pointer, reinterpreted to `int` — see [`build_funcbox`]) instead of a null one:
    /// the lambda's own lifted function reads it back apart at its own prologue (see
    /// `Analyzer::hir_begin_function`). Drops coverage if the name is not a registered function def.
    /// The funcbox owns a retain on `env_cell` (via `$funcbox_new`); leftover of the cell is
    /// released only in the same leftover batch as the box.
    pub(in crate::analyzer) fn hir_set_capturing_func_value(
        &mut self,
        name: &str,
        env_cell: HExpr,
        func_ty: &Type,
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
        let int_ty = self.type_ctx.interner.int();
        let ret_ty = self.type_ctx.lower(ret);
        let raw = HExpr::new(
            int_ty,
            HExprKind::Var(Binding::Func(Callee {
                def,
                instance: vec![],
                ret: ret_ty,
                take_params: vec![],
            })),
        );
        let env_int = HExpr::new(int_ty, HExprKind::Cast(Box::new(env_cell)));
        self.hir.last = self.build_funcbox(raw, Some(env_int), func_ty);
    }

    /// Like [`hir_set_capturing_func_value`], but for **two or more** captured names: the
    /// environment is an `object[]` array (one slot per capture, in `env_cells`' order) rather than
    /// a single `CaptureCell<T>` — see the lifted function's receiving half,
    /// `Analyzer::receive_closure_captures`. Each cell is written into the array as an ordinary
    /// `object[]` store, so the emitter's normal container-store rule retains it on the array's
    /// behalf (see `mir::passes::rc`'s doc comment). The array itself is owned by the funcbox
    /// (`$funcbox_new` retains it); leftover of `__closure_env_array` last-drops it only in the
    /// same batch as the box (typed `TAG_CLOSURE_ENV`).
    pub(in crate::analyzer) fn hir_set_multi_capturing_func_value(
        &mut self,
        name: &str,
        env_cells: Vec<HExpr>,
        func_ty: &Type,
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
        let int_ty = self.type_ctx.interner.int();
        let object_ty = self.type_ctx.interner.object();
        let array_ty = self.type_ctx.interner.array(object_ty);
        let len = HExpr::new(int_ty, HExprKind::IntLit(env_cells.len() as i64));
        let array_new = HExpr::new(
            array_ty,
            HExprKind::ArrayNew {
                elem_ty: object_ty,
                len: Box::new(len),
                closure_env: true,
            },
        );
        let array_local = LocalId(self.hir.next_local);
        self.hir.next_local += 1;
        self.hir.local_decls.push(HLocal {
            id: array_local,
            name: dream_abi::intrinsics::CLOSURE_ENV_ARRAY_LOCAL.to_string(),
            ty: array_ty,
        });
        self.push_stmt(HStmt::Let {
            local: array_local,
            ty: array_ty,
            value: array_new,
        });
        let array_read = || HExpr::new(array_ty, HExprKind::Var(Binding::Local(array_local)));
        for (i, cell) in env_cells.into_iter().enumerate() {
            let index = HExpr::new(int_ty, HExprKind::IntLit(i as i64));
            let value = HExpr::new(object_ty, HExprKind::Cast(Box::new(cell)));
            self.push_stmt(HStmt::Assign {
                place: HPlace::Index {
                    array: Box::new(array_read()),
                    index: Box::new(index),
                },
                value,
            });
        }

        let ret_ty = self.type_ctx.lower(ret);
        let raw = HExpr::new(
            int_ty,
            HExprKind::Var(Binding::Func(Callee {
                def,
                instance: vec![],
                ret: ret_ty,
                take_params: vec![],
            })),
        );
        let env_int = HExpr::new(int_ty, HExprKind::Cast(Box::new(array_read())));
        self.hir.last = self.build_funcbox(raw, Some(env_int), func_ty);
    }
}
