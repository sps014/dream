use super::*;
use dream_types::{TyKind, TypeId};

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn ensure_array_collection(
        &mut self,
        ty: TypeId,
        diagnostics: &mut DiagnosticBag,
    ) {
        let TyKind::Array(elem) = self.type_ctx.interner.kind(ty) else {
            return;
        };
        let args = vec![self.type_ctx.syntax_type(*elem)];
        if !self.array_collections_attached.insert(ty) {
            return;
        }
        let extensions = self
            .generic_extends
            .get(&GenericExtendTarget::Array)
            .cloned()
            .unwrap_or_default();
        self.register_generic_extension_methods(GenericExtendTarget::Array, ty, &args, diagnostics);
        for extension in extensions {
            let scope = self.type_ctx.scope();
            self.type_ctx
                .set_scope(self.graph.module_for_file(extension.file_path.as_deref()));
            let bindings = generic_bindings(
                extension.generic_parameters.as_deref().unwrap_or(&[]),
                &args,
            );
            let implements: Vec<_> = extension
                .implements
                .iter()
                .map(|iface| substitute_generic_type(iface, &bindings))
                .collect();
            self.validate_implements(
                ty,
                &implements,
                &extension.methods,
                &bindings,
                extension.target.position,
                diagnostics,
            );
            self.type_ctx.set_scope(scope);
        }
        let interfaces = self.implemented_interfaces(ty).cloned().unwrap_or_default();
        let mut defaults = IndexMap::new();
        for iface in interfaces {
            for method in self
                .interface_method_list(iface)
                .cloned()
                .unwrap_or_default()
            {
                let member = accessor_member_name(method);
                if method.is_default_impl
                    && !method.is_static
                    && !self
                        .function_table
                        .methods
                        .contains_key(&(ty, member.clone()))
                {
                    defaults.entry(member).or_insert(method);
                }
            }
        }
        for method in defaults.into_values() {
            let mut method = method.clone();
            method.is_default_impl = false;
            let method = self.arena.alloc(method);
            self.register_methods_for(
                ty,
                std::slice::from_ref(method),
                &GenericBindings::new(),
                diagnostics,
            );
        }
    }
}
