use super::*;

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn analyze_pgm(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<SemanticInfo<'_>, SemanticError> {
        let mut symbol_table_map = HashMap::new();
        self.type_ctx.set_scope(dream_types::ModuleId::ROOT);
        self.type_ctx
            .register(DefKind::Struct, FUTURE_TYPE, vec!["T".to_string()]);
        for file in &self.graph.files {
            self.type_ctx.set_scope(file.module);
            for declaration in &file.program.structs {
                let def = self.type_ctx.register(
                    DefKind::Struct,
                    &declaration.name.text,
                    generic_param_names(&declaration.generic_parameters),
                );
                self.record_ide_definition(def, &declaration.name, Some(&file.path));
                for field in &declaration.fields {
                    self.record_ide_member_definition(def, &field.name, Some(&file.path));
                }
            }
            for declaration in &file.program.enums {
                let kind = if declaration.is_data_enum() {
                    DefKind::Union
                } else {
                    DefKind::Enum
                };
                let def = self.type_ctx.register(
                    kind,
                    &declaration.name.text,
                    generic_param_names(&declaration.generic_parameters),
                );
                self.record_ide_definition(def, &declaration.name, Some(&file.path));
                for variant in &declaration.variants {
                    self.record_ide_member_definition(def, &variant.name, Some(&file.path));
                }
            }
            for declaration in &file.program.interfaces {
                let def = self.type_ctx.register(
                    DefKind::Interface,
                    &declaration.name.text,
                    generic_param_names(&declaration.generic_parameters),
                );
                self.record_ide_definition(def, &declaration.name, Some(&file.path));
            }
        }

        // Stash generic `extend` templates before any type instantiation can occur (a concrete
        // union/struct field may instantiate a generic union during `register_enums`), so the
        // extension methods are always available to attach at the first instantiation.
        self.stash_generic_extensions(node);
        self.register_enums(node, diagnostics);
        // Interfaces are registered before structs so a class's implements clause can be validated
        // against the interface method signatures during struct registration.
        self.register_interfaces(node, diagnostics);
        self.register_structs(node, diagnostics);
        self.register_extensions(node, diagnostics);
        self.register_functions(node, diagnostics);
        // `ref struct` params on an `async` function/method would need to survive a suspend point,
        // which spills the function's live locals into a heap-allocated coroutine state object —
        // exactly the escape a `ref struct` forbids. Checked once every function/method/`extend`
        // signature above is registered.
        self.check_ref_struct_async_boundary(node, diagnostics);
        // Aliased `import a.b.c as x;` resolve against the now-fully-registered function table
        // (cross-module collisions have already been promoted to their module-qualified keys), but
        // must land before body analysis so every call site can see the alias.
        self.register_import_aliases(diagnostics);
        // Globals are analyzed after functions/types are known (so initializers can call them) but
        // before function bodies, so those bodies can resolve global identifiers.
        // HIR global slots are assigned incrementally inside `register_globals` (in declaration
        // order) so both later initializers and function bodies can resolve global identifiers.
        self.register_globals(node, diagnostics);
        self.analyze_function_bodies(node, &mut symbol_table_map, diagnostics)?;
        self.analyze_pending_instantiations(&mut symbol_table_map, diagnostics)?;

        // Inferred receiver exclusivity: classify every method's `this` contract once bodies
        // are fully analyzed and all types are registered. Runs only on otherwise-clean
        // programs — a poisoned program fails before codegen anyway.
        if !diagnostics.has_errors() {
            self.classify_receiver_modes(node, diagnostics);
            self.check_borrow_collisions(node, diagnostics);
            self.check_closure_self_capture(node, diagnostics);
        }

        // Per-statement/expression analysis recovers locally (reporting into the bag and poisoning
        // with `Type::Unknown`) so every independent error in the program is surfaced. The typed
        // boundary failure is raised once here, from the aggregate error state, so the driver can
        // abort before code generation.
        if diagnostics.has_errors() {
            return Err(SemanticError::AnalysisFailed);
        }

        // Built before the borrow-immutable `SemanticInfo` literal below, since lowering field types
        // needs `&mut self.type_ctx`.
        let layouts = self.hir_build_layouts();
        let object_methods = layouts
            .structs
            .keys()
            .filter(|ty| self.struct_table.get_struct(**ty).is_some())
            .chain(layouts.unions.keys().filter(|ty| self.union_table.contains_key(*ty)))
            .map(|&ty| {
                (
                    ty,
                    dream_hir::ObjectMethods {
                        to_string: self.unique_method_def(ty, dream_abi::intrinsics::TO_STRING),
                        hash_code: self.unique_method_def(ty, dream_abi::intrinsics::HASH_CODE),
                    },
                )
            })
            .collect();
        self.validate_layout_case_labels(&layouts, diagnostics);
        if diagnostics.has_errors() {
            return Err(SemanticError::AnalysisFailed);
        }
        let type_names = self.hir_build_type_names(&layouts);
        let imports = self.hir_build_imports(node);
        let intrinsics = self.hir_build_intrinsics(node);
        let interfaces = self.hir_build_interfaces();
        let hir_functions = std::mem::take(&mut self.hir.functions);
        let hir_globals = std::mem::take(&mut self.hir.global_decls);
        let type_symbols = self
            .type_ctx
            .interner
            .iter_kinds()
            .map(|(id, _)| {
                (
                    id,
                    dream_types::type_symbol(&self.type_ctx.interner, &self.type_ctx.defs, id),
                )
            })
            .collect();

        let enum_entries: Vec<(dream_types::DefId, indexmap::IndexMap<String, i32>)> = self
            .enum_table
            .iter()
            .map(|(n, m)| (*n, m.clone()))
            .collect();
        let mut hir_enums = indexmap::IndexMap::new();
        for (def, members) in enum_entries {
            let tid = self.type_ctx.interner.enum_ty(def);
            let mems: Vec<(String, i32)> = members.iter().map(|(n, v)| (n.clone(), *v)).collect();
            hir_enums.insert(tid, (self.type_ctx.defs.name(def).to_string(), mems));
        }

        Ok(SemanticInfo {
            hash_map: symbol_table_map,
            function_table: &self.function_table,
            struct_table: &self.struct_table,
            instantiated_generics: self.instantiated_generics.clone(),
            struct_methods: self.struct_methods.clone(),
            enums: self.enum_table.clone(),
            unions: self.union_table.clone(),
            globals: self.globals.clone(),
            hir: dream_hir::Hir {
                functions: hir_functions,
                globals: hir_globals,
                instances: vec![],
                layouts,
                imports,
                intrinsics,
                interfaces,
                enums: hir_enums,
                type_names,
                type_symbols,
                object_methods,
            },
        })
    }
}
