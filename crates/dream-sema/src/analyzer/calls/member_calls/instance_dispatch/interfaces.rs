use super::*;

impl<'a> Analyzer<'a> {
    /// If `obj_type` names an interface, returns that interface's name; otherwise `None`.
    pub(crate) fn interface_receiver_name(&self, obj_type: &Type) -> Option<String> {
        let name = obj_type.get_type();
        if self.is_interface_name(&name) {
            Some(name)
        } else {
            None
        }
    }

    /// Dispatches a method call on an interface-typed receiver. Resolves `method` against the
    /// interface's ordered signature list (yielding its local slot index and return type),
    /// type-checks the arguments, and emits a dynamically-dispatched `InterfaceCall` HIR node.
    pub(crate) fn analyze_interface_method(
        &mut self,
        iface_name: &str,
        method: &SyntaxToken,
        params: &Vec<ExpressionNode<'a>>,
        ctx: &super::super::super::AnalyzerContext<'a, '_>,
        receiver: Option<dream_hir::HExpr>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let (arg_types, arg_hirs) = self.analyze_call_arguments(
            params,
            ctx.parent_function,
            ctx.symbol_table,
            diagnostics,
        )?;

        let methods = self
            .interface_methods
            .get(iface_name)
            .cloned()
            .unwrap_or_default();
        let Some((slot, im)) = methods
            .iter()
            .enumerate()
            .find(|(_, m)| accessor_member_name(m) == method.text || m.name.text == method.text)
        else {
            return Err(report(
                diagnostics,
                format!(
                    "interface '{}' has no method '{}'",
                    self.ty_str_display(iface_name),
                    method.text
                ),
                Some(method.position),
            ));
        };

        let expected: Vec<String> = im.parameters.iter().map(|p| p.type_.get_type()).collect();
        // Calling an `async` interface method is eager and yields a `Future<T>` handle (just like an
        // async instance method); the concrete implementation dispatches to a `Future`-producing
        // constructor. The caller must `await` the result.
        let ret_type = Self::async_return_type(im.is_async, im.return_type.clone());
        if expected.len() != arg_types.len() {
            diagnostics.report_error(
                format!(
                    "interface method '{}.{}' expects {} arguments, got {}",
                    self.ty_str_display(iface_name),
                    method.text,
                    expected.len(),
                    arg_types.len()
                ),
                Some(method.position),
            );
            self.hir_none();
            return Ok(ret_type);
        }
        for (i, given) in arg_types.iter().enumerate() {
            if !self.type_str_assignable(&expected[i], given) {
                diagnostics.report_error(
                    format!(
                        "interface method '{}.{}' expects parameter {} to be {}, got {}",
                        self.ty_str_display(iface_name),
                        method.text,
                        i + 1,
                        self.ty_str_display(&expected[i]),
                        self.ty_str_display(given)
                    ),
                    Some(method.position),
                );
            }
        }

        let iface_id = self.interface_methods.get_index_of(iface_name).unwrap_or(0);
        // The `call_indirect` signature is `fun(this, params...): ret`, with `this` typed as
        // `object` (an `i32` pointer, matching every concrete implementation's receiver).
        let sig = self.interface_dispatch_sig(im);
        self.hir_set_interface_call(receiver, iface_id, slot, sig, arg_hirs, &ret_type);
        Ok(ret_type)
    }

    /// Interns the `fun(this, params...): ret` function type used to `call_indirect` an interface
    /// method: `this` is `object` (a tagged pointer), followed by the method's declared parameters
    /// and its return type. The same signature is used to declare the WASM `call_indirect` type.
    pub(crate) fn interface_dispatch_sig(
        &mut self,
        method: &FunctionNode<'a>,
    ) -> dream_types::TypeId {
        let mut params = vec![self.type_ctx.interner.object()];
        for p in &method.parameters {
            let id = self.type_ctx.lower(&p.type_);
            params.push(id);
        }
        // An `async` interface method dispatches to a concrete async constructor whose WASM result
        // is the `Future` frame pointer (an `i32`), so the `call_indirect` signature returns an
        // `object`-shaped pointer regardless of the method's declared return type.
        let ret = if method.is_async {
            self.type_ctx.interner.object()
        } else {
            match &method.return_type {
                Some(t) => self.type_ctx.lower(t),
                None => self.type_ctx.interner.void(),
            }
        };
        self.type_ctx.interner.func(params, ret)
    }
}
