use super::*;

impl<'a> Analyzer<'a> {
    /// Records an indirect call `f(args)` where `f` is a function-typed local: unboxes `f` (see
    /// `build_funcbox`) — publishing its environment word to `$__closure_env` (read by a capturing
    /// callee's own prologue; see `hir_read_closure_env`) and extracting its function-table index —
    /// then dispatches through that index. `f` is read into a fresh local first so both extractions
    /// (env, funcidx) observe the same box value without re-evaluating `f` twice. Drops coverage if
    /// the name is not a known local, the `Closure` intrinsics are unavailable, or any argument is
    /// not representable.
    pub(in crate::analyzer) fn hir_set_indirect_call(
        &mut self,
        name: &str,
        args: Vec<Option<HExpr>>,
        ret: &Type,
    ) {
        if !self.active() {
            self.hir.last = None;
            return;
        }
        let Some(&(local, ty)) = self.hir.locals.get(name) else {
            self.hir.last = None;
            return;
        };
        // A captured `fun(...)`-typed name (`self.hir.boxed`, see `hir_set_var`'s doc comment)
        // reads through its `CaptureCell<T>` box's `.value` field: `ty` here is the *cell's* type, not
        // the `fun(...)` shape `hir_set_indirect_call_expr` needs to pick the right
        // `call_indirect` signature — dereference it exactly like a plain read would.
        let target = if let Some(&elem_ty) = self.hir.boxed.get(name) {
            let obj = HExpr::new(ty, HExprKind::Var(Binding::Local(local)));
            HExpr::new(
                elem_ty,
                HExprKind::Field {
                    obj: Box::new(obj),
                    field: 0,
                },
            )
        } else {
            HExpr::new(ty, HExprKind::Var(Binding::Local(local)))
        };
        self.hir_set_indirect_call_expr(target, args, ret);
    }

    pub(in crate::analyzer) fn is_place_read(e: &HExpr) -> bool {
        match &e.kind {
            HExprKind::Var(Binding::Local(_)) | HExprKind::Var(Binding::Global(_)) => true,
            HExprKind::Field { obj, .. } => Self::is_place_read(obj),
            _ => false,
        }
    }

    /// Shared unboxing logic for an indirect call through a boxed `fun(...)` value `boxed` — see
    /// [`hir_set_indirect_call`]. Used for both named locals and arbitrary `fun(...)`-typed
    /// expression callees.
    ///
    /// When `boxed` is already a local, both `funcbox_env` / `funcbox_funcidx` reads use that local
    /// directly (no extra ARC). Complex callees are materialized into a temporary `__closure_box`
    /// (borrowed retain) and that temporary is cleared to null after the call so the retain does
    /// not keep the funcbox alive until function exit — otherwise a loop that calls `f()` each
    /// iteration would leak the last (or every) closure via the stale temp.
    pub(in crate::analyzer) fn hir_set_indirect_call_expr(
        &mut self,
        boxed: HExpr,
        args: Vec<Option<HExpr>>,
        ret: &Type,
    ) {
        let (Some(funcidx_def), Some(env_def)) = (
            self.closure_intrinsic("funcbox_funcidx"),
            self.closure_intrinsic("funcbox_env"),
        ) else {
            self.hir.last = None;
            return;
        };
        let Some(collected) = Self::collect_hir_args(args) else {
            self.hir.last = None;
            return;
        };
        let int_ty = self.type_ctx.interner.int();
        let box_ty = boxed.ty;

        // A place read (a local, or a captured name's `CaptureCell.value`) can be read twice. The
        // scratch path cannot express a `void` call as a value, so it must stay for real temporaries.
        let (box_expr, scratch_local) = if Self::is_place_read(&boxed) {
            (boxed, None)
        } else {
            let box_local = LocalId(self.hir.next_local);
            self.hir.next_local += 1;
            self.hir.local_decls.push(HLocal {
                id: box_local,
                name: "__closure_box".to_string(),
                ty: box_ty,
            });
            self.push_stmt(HStmt::Let {
                local: box_local,
                ty: box_ty,
                value: boxed,
            });
            (
                HExpr::new(box_ty, HExprKind::Var(Binding::Local(box_local))),
                Some(box_local),
            )
        };

        let Some(&(env_global, _)) = self.hir.globals.get("__closure_env") else {
            self.hir.last = None;
            return;
        };
        let env_call = HExpr::new(
            int_ty,
            HExprKind::Call {
                callee: Callee {
                    def: env_def,
                    instance: vec![],
                    ret: int_ty,
                    take_params: vec![],
                },
                args: vec![box_expr.clone()],
            },
        );
        self.push_stmt(HStmt::Assign {
            place: HPlace::Global(env_global),
            value: env_call,
        });

        // Funcidx is a plain `int` at runtime. The `fun(...)` shape for `call_indirect` lives on
        // `IndirectCall.sig` (not on `target`'s type) so ARC does not release a table index as a
        // funcbox when `TyKind::Func` is a reference.
        let funcidx_call = HExpr::new(
            int_ty,
            HExprKind::Call {
                callee: Callee {
                    def: funcidx_def,
                    instance: vec![],
                    ret: int_ty,
                    take_params: vec![],
                },
                args: vec![box_expr],
            },
        );
        let ret_ty = self.type_ctx.lower(ret);
        let call = HExpr::new(
            ret_ty,
            HExprKind::IndirectCall {
                target: Box::new(funcidx_call),
                sig: box_ty,
                args: collected,
            },
        );

        if let Some(box_local) = scratch_local {
            // Drop the scratch retain immediately after the call so the funcbox's lifetime follows
            // the source expression, not the enclosing function.
            let clear = HExpr::new(box_ty, HExprKind::IntLit(0));
            if matches!(
                self.type_ctx.interner.kind(ret_ty),
                dream_types::TyKind::Void
            ) {
                self.push_stmt(HStmt::Expr(call));
                self.push_stmt(HStmt::Assign {
                    place: HPlace::Local(box_local),
                    value: clear,
                });
                self.hir.last = None;
                self.mark_void_emitted();
            } else {
                let result_local = LocalId(self.hir.next_local);
                self.hir.next_local += 1;
                self.hir.local_decls.push(HLocal {
                    id: result_local,
                    name: "__indirect_result".to_string(),
                    ty: ret_ty,
                });
                self.push_stmt(HStmt::Let {
                    local: result_local,
                    ty: ret_ty,
                    value: call,
                });
                self.push_stmt(HStmt::Assign {
                    place: HPlace::Local(box_local),
                    value: clear,
                });
                self.hir.last = Some(HExpr::new(
                    ret_ty,
                    HExprKind::Var(Binding::Local(result_local)),
                ));
            }
        } else {
            self.hir.last = Some(call);
        }
    }
}
