//! Monomorphizing a generic free function: registering one concrete instance (for a call) and
//! instantiating one for use as a first-class function value.

use super::*;
use crate::function_table::FunctionTableInfo;

impl<'a> Analyzer<'a> {
    pub(crate) fn register_generic_function_instance(
        &mut self,
        template: &'a FunctionNode<'a>,
        bindings: &GenericBindings,
    ) -> crate::function_table::FunctionIdentity {
        let old_scope = self.type_ctx.scope();
        self.type_ctx.set_scope(self.graph.module_for_file(template.file_path.as_deref()));
        let def = self.function_table.declaration_node(template).map(|key| key.0)
            .or_else(|| self.type_ctx.resolve(DefKind::Function, &template.name.text));
        let Some(def) = def else {
            self.type_ctx.set_scope(old_scope);
            return (self.type_ctx.register(DefKind::Function, &template.name.text, generic_param_names(&template.generic_parameters)), Vec::new());
        };
        let args: Vec<_> = bindings.values().map(|ty| self.type_ctx.lower(ty)).collect();
        let identity = (def, args);
        if !self.function_table.functions.contains_key(&identity) {
            let mut specialized = template.clone();
            Self::substitute_generic_signature(&mut specialized, bindings);
            let specialized_ref: &'a FunctionNode<'a> = self.arena.alloc(specialized);
            let mut info = FunctionTableInfo::from_identity(specialized_ref, identity.clone(), &mut self.type_ctx);
            info.declaring_module = self.module_of(template.file_path.as_ref());
            self.instantiated_generics.insert(identity.clone(), (bindings.clone(), specialized_ref));
            self.function_table.record_declaration(specialized_ref, identity.clone());
            self.function_table.add_instance(info, def, identity.1.clone());
            let owners: Vec<_> = self.function_table.generic_methods.iter()
                .filter(|(_, candidate)| **candidate == def).map(|(key, _)| key.clone()).collect();
            for key in owners {
                self.function_table.methods.entry(key).or_default().push(identity.clone());
            }
        }
        self.type_ctx.set_scope(old_scope);
        identity
    }

    /// Instantiates a generic free function used as a first-class *value* (`let cmp: fun(T, T): int =
    /// natural_order;`). The concrete type arguments are inferred by unifying the template's declared
    /// parameter/return types with the `expected` function type at the use site; the instance is
    /// registered (see `register_generic_function_instance`) and a `FuncValue` referencing its
    /// mangled name is emitted. Returns the monomorphized function type, or `None` (with a
    /// diagnostic) if there is no function-typed context to infer from.
    pub(crate) fn instantiate_generic_function_value(
        &mut self,
        id: &SyntaxToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Option<Type> {
        let def = self.type_ctx.resolve(DefKind::Function, &id.text)?;
        let template = *self.generic_functions.get(&def)?;

        // The expected type at this site drives inference; it must be a concrete function type.
        let expected = self
            .current_expected_type
            .as_ref()
            .map(|t| Self::monomorphize_type(t, &self.current_generic_bindings));
        let Some(Type::Function(exp_params, exp_ret)) = expected else {
            // Callers that want a polymorphic binding use `Type::GenericFunctionItem` instead of
            // this helper (see `analyze_identifier`).
            diagnostics.report_error(
                format!(
                    "generic function '{}' can only be used as a concrete value in a context with a known function type (e.g. `let f: fun(int, int): int = {};`)",
                    id.text, id.text
                ),
                Some(id.position),
            );
            return None;
        };

        // Infer bindings by matching the expected parameter types against the template's formals,
        // then verify the type parameters' constraints are satisfied by those concrete types.
        let param_ids: Vec<_> = exp_params.iter().map(|p| self.type_ctx.lower(p)).collect();
        let bindings =
            self.infer_generic_bindings(template, &None, &param_ids, &id.position, diagnostics);
        self.verify_generic_constraints(
            &template.generic_constraints,
            &bindings,
            &id.position,
            diagnostics,
        );

        let identity = self.register_generic_function_instance(template, &bindings);
        // The func value must reference the base template's `DefId` + concrete instance args (in
        // binding order) so it maps to the monomorphized instance's function-table slot.
        let ret = (*exp_ret).clone();
        let func_ty = Type::Function(exp_params, exp_ret);
        self.hir_set_func_value_identity(&identity, &func_ty, &ret);
        Some(func_ty)
    }
}
