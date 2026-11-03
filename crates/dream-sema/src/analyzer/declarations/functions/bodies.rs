use super::*;

impl<'a> Analyzer<'a> {
    /// Pass 2: analyze the body of every concrete function.
    pub(in crate::analyzer) fn analyze_function_bodies(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        for function in node.functions.iter() {
            if function.generic_parameters.is_some() {
                continue;
            }
            // Extern functions have no body; their signature is enough for call-site checks.
            if function.is_extern {
                continue;
            }
            if self.defer_library_body(function, &GenericBindings::new()) {
                continue;
            }
            diagnostics.file_path = file_path_string(&function.file_path);
            self.analyze_function(function, diagnostics)?;
        }
        Ok(())
    }

    /// Passes 3 & 4 (combined fixpoint): analyze the bodies of every monomorphized generic
    /// function instance, every (de-sugared) struct method, and every deferred library body that
    /// emitted HIR reaches.
    ///
    /// Analyzing one body can lazily instantiate *more* generics — a struct method that uses
    /// `List<JsonValue>` queues new struct methods, and a builder that calls `List<JsonValue>()`
    /// queues a new generic function instance. The two feed each other, so we loop until neither
    /// the generic-function set nor the struct-method list grows. Both instantiation paths are
    /// idempotent (guarded by the struct/function tables), so this terminates.
    pub(in crate::analyzer) fn analyze_pending_instantiations(
        &mut self,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let _phase = tracing::info_span!("compile_phase", phase = "monomorphization").entered();
        let mut processed_generics: indexmap::IndexSet<crate::function_table::FunctionIdentity> =
            indexmap::IndexSet::new();
        let mut method_index = 0;
        let mut reach = super::reach::LibraryReach::default();
        // Semantic type depth bounds recursive expansion without penalizing qualified callback names.
        let mut items_processed: usize = 0;
        loop {
            let mut progressed = false;

            // Monomorphized generic function instances (e.g. `List<JsonValue>`, `swap_int_string`).
            let pending: Vec<crate::function_table::FunctionIdentity> = self
                .instantiated_generics
                .keys()
                .filter(|k| !processed_generics.contains(*k))
                .cloned()
                .collect();
            for mangled_name in pending {
                processed_generics.insert(mangled_name.clone());
                let (bindings, template) = match self.instantiated_generics.get(&mangled_name) {
                    Some((b, t)) => (b.clone(), *t),
                    None => continue,
                };
                diagnostics.file_path = file_path_string(&template.file_path);
                self.with_generic_bindings(bindings.clone(), |s| {
                    s.analyze_function(template, diagnostics)
                })?;
                progressed = true;
                items_processed += 1;
                check_instantiation_bounds(
                    items_processed,
                    &bindings,
                    &self
                        .function_table
                        .emitted_name(&self.type_ctx, &mangled_name),
                    diagnostics,
                )?;
            }

            // Arrow-lambdas lowered to synthesized top-level functions (see `expressions::lambda`).
            // The lambda literal itself is never generic in v1, but the *enclosing* method it was
            // written in might be (e.g. a lambda inside a `Task.map<T, TOut>` method) - re-apply
            // the bindings captured at its use site so its body sees the same substitution.
            let pending_lambdas: Vec<dream_types::DefId> = self
                .pending_lambdas
                .keys()
                .filter(|k| !processed_generics.contains(&(**k, Vec::new())))
                .cloned()
                .collect();
            for name in pending_lambdas {
                processed_generics.insert((name, Vec::new()));
                items_processed += 1;
                let (template, bindings) = match self.pending_lambdas.get(&name) {
                    Some((t, b)) => (*t, b.clone()),
                    None => continue,
                };
                check_instantiation_bounds(
                    items_processed,
                    &bindings,
                    self.type_ctx.defs.name(name),
                    diagnostics,
                )?;
                diagnostics.file_path = file_path_string(&template.file_path);
                self.with_generic_bindings(bindings.clone(), |s| {
                    s.analyze_function(template, diagnostics)
                })?;
                progressed = true;
            }

            // De-sugared struct methods, including those for newly instantiated generic structs.
            while method_index < self.struct_methods.len() {
                let (method, bindings) = self.struct_methods[method_index].clone();
                method_index += 1;
                if self.defer_library_body(method, &bindings) {
                    continue;
                }
                diagnostics.file_path = file_path_string(&method.file_path);
                self.with_generic_bindings(bindings.clone(), |s| {
                    s.analyze_function(method, diagnostics)
                })?;
                let Some(identity) = self.function_declaration(method) else {
                    continue;
                };
                let key = self.function_table.emitted_name(&self.type_ctx, &identity);
                items_processed += 1;
                check_instantiation_bounds(items_processed, &bindings, &key, diagnostics)?;
                progressed = true;
            }

            // Runs after the struct-method pass so the methods of types instantiated by the bodies
            // it analyzes are already deferred when the next round scans those bodies.
            progressed |= self.analyze_reachable_library_bodies(&mut reach, diagnostics)?;

            if !progressed {
                break;
            }
        }
        Ok(())
    }
}
