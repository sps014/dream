use super::*;

/// A struct lowered for layout: `(interned type id, name, packed, [(field name, interned field type)])`.
type LoweredStruct = (TypeId, String, bool, Vec<(String, TypeId, bool, bool)>);
/// Resolved fields retain type-argument provenance across module boundaries.
type StructFieldSnapshot = (String, TypeId, bool, bool);

impl<'a> Analyzer<'a> {
    /// Turns on HIR collection so a top-level variable's initializer expression is captured while it
    /// is analyzed. There is no enclosing function, so there are no locals/blocks — only the top
    /// expression's HIR is wanted. Paired with [`Self::hir_global_init_finish`].
    pub(in crate::analyzer) fn hir_global_init_begin(&mut self) {
        self.hir.collecting = true;
        self.hir.ok = true;
        self.hir.last = None;
    }

    /// Stores the captured initializer for global `name` (if it was fully representable) and turns
    /// collection back off.
    pub(in crate::analyzer) fn hir_global_init_finish(&mut self, name: &str) {
        if self.hir.collecting && self.hir.ok {
            if let Some(init) = self.hir.last.take() {
                self.hir.pending_global_inits.insert(name.to_string(), init);
            }
        }
        self.hir.collecting = false;
        self.hir.last = None;
    }

    /// Registers one top-level variable's HIR slot as it is analyzed (in declaration order), so a
    /// *later* global's initializer can resolve an *earlier* global to a [`Binding::Global`]. The
    /// slot `id` must equal the variable's index in [`Analyzer::globals`]. The initializer captured
    /// by [`Self::hir_global_init_finish`] (if representable) is attached to the surfaced [`HGlobal`].
    pub(in crate::analyzer) fn hir_register_global(
        &mut self,
        name: &str,
        type_str: &str,
        is_const: bool,
    ) {
        let ty = self
            .type_ctx
            .resolved_type(type_str)
            .unwrap_or_else(|| self.type_ctx.interner.error());
        let id = GlobalId(self.hir.globals.len() as u32);
        self.hir.globals.insert(name.to_string(), (id, ty));
        let init = self.hir.pending_global_inits.shift_remove(name);
        self.hir.global_decls.push(HGlobal {
            id,
            name: name.to_string(),
            ty,
            is_const,
            init,
        });
    }

    /// Builds the [`dream_hir::LayoutTable`] from the analyzed struct and union tables: each struct's
    /// `DefId` maps to its field offsets/sizes, and each union's `DefId` to its per-variant
    /// discriminant + payload offsets, so the backend can lower `obj.field` reads and `new`/variant
    /// construction to concrete loads/stores.
    pub(in crate::analyzer) fn hir_build_layouts(&mut self) -> dream_hir::LayoutTable {
        use dream_hir::{
            LayoutFieldDef, LayoutTable, StructLayoutDef, UnionLayoutDef, UnionVariantDef,
        };
        // Snapshot field types in declaration order first, so `type_ctx` can be re-borrowed mutably
        // for lowering without aliasing the struct/union-table borrows.
        // Discriminated unions are also registered in the struct table (for tagging/release), but they
        // get a variant-aware layout + `to_string` from the union table below — so exclude them here to
        // avoid a duplicate (empty) struct layout and a duplicate `$<Union>_to_string`.
        let struct_snapshot: Vec<(TypeId, String, bool, Vec<StructFieldSnapshot>)> = self
            .struct_table
            .structs
            .iter()
            .filter(|(ty, _)| {
                matches!(
                    self.type_ctx.interner.kind(**ty),
                    dream_types::TyKind::Struct(..)
                )
            })
            .map(|(ty, info)| {
                let fields = info
                    .fields
                    .iter()
                    .map(|(fname, f)| (fname.clone(), f.ty, f.is_weak, f.is_unowned))
                    .collect();
                (*ty, info.name.clone(), info.packed, fields)
            })
            .collect();
        type VariantSnap = (String, i32, Vec<(String, TypeId)>);
        let union_snapshot: Vec<(TypeId, String, Vec<VariantSnap>)> = self
            .union_table
            .iter()
            .map(|(ty, info)| {
                let variants = info
                    .variants
                    .iter()
                    .map(|v| {
                        let fields = v.fields.iter().map(|f| (f.name.clone(), f.ty)).collect();
                        (v.name.clone(), v.discriminant, fields)
                    })
                    .collect();
                (*ty, info.name.clone(), variants)
            })
            .collect();

        // Field IDs are producer facts; re-resolving their source names here loses foreign arguments.
        let mut lowered: Vec<LoweredStruct> = Vec::with_capacity(struct_snapshot.len());
        for (ty, name, packed, fields) in struct_snapshot {
            if let dream_types::TyKind::Struct(def, _) = self.type_ctx.interner.kind(ty) {
                self.type_ctx.set_scope(def.module);
            }
            let defs: Vec<(String, TypeId, bool, bool)> = fields
                .iter()
                .map(|(fname, t, is_weak, is_unowned)| (fname.clone(), *t, *is_weak, *is_unowned))
                .collect();
            lowered.push((ty, name, packed, defs));
        }
        // Tuples are inline aggregates and participate in the same target-specific table.
        let tuple_defs: Vec<(TypeId, Vec<TypeId>)> = self
            .type_ctx
            .interner
            .iter_kinds()
            .filter_map(|(id, kind)| match kind {
                dream_types::TyKind::Tuple(elems) => Some((id, elems.clone())),
                _ => None,
            })
            .collect();
        let mut struct_defs = Vec::new();
        for (ty, name, packed, defs) in lowered {
            if let dream_types::TyKind::Struct(def, _) = self.type_ctx.interner.kind(ty) {
                self.type_ctx.set_scope(def.module);
            }
            let destructor = if self
                .struct_table
                .get_struct(ty)
                .is_some_and(|info| info.has_destructor)
            {
                self.type_ctx.resolve(
                    DefKind::Function,
                    &dream_types::method_fn(&name, dream_syntax::nodes::types::DESTRUCTOR_NAME),
                )
            } else {
                None
            };
            let name = if self.type_ctx.scope() == dream_types::ModuleId::ROOT {
                name
            } else {
                format!(
                    "_Dt{}",
                    dream_types::type_symbol(&self.type_ctx.interner, &self.type_ctx.defs, ty)
                )
            };
            struct_defs.push(StructLayoutDef {
                ty,
                name,
                fields: defs
                    .into_iter()
                    .map(|(name, ty, is_weak, is_unowned)| LayoutFieldDef {
                        name,
                        ty,
                        is_weak,
                        is_unowned,
                    })
                    .collect(),
                packed,
                destructor,
            });
        }
        for (ty, elems) in tuple_defs {
            let name = dream_types::display_name(&self.type_ctx.interner, &self.type_ctx.defs, ty);
            let safe_name: String = name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let defs: Vec<(String, TypeId, bool, bool)> = elems
                .iter()
                .enumerate()
                .map(|(i, e)| (i.to_string(), *e, false, false))
                .collect();
            struct_defs.push(StructLayoutDef {
                ty,
                name: safe_name,
                fields: defs
                    .into_iter()
                    .map(|(name, ty, is_weak, is_unowned)| LayoutFieldDef {
                        name,
                        ty,
                        is_weak,
                        is_unowned,
                    })
                    .collect(),
                packed: false,
                destructor: None,
            });
        }
        let mut union_defs = Vec::new();
        for (ty, name, variants) in union_snapshot {
            if let dream_types::TyKind::Union(def, _) = self.type_ctx.interner.kind(ty) {
                self.type_ctx.set_scope(def.module);
            }
            union_defs.push(UnionLayoutDef {
                ty,
                name,
                variants: variants
                    .into_iter()
                    .map(|(name, discriminant, fields)| UnionVariantDef {
                        name,
                        discriminant,
                        fields: fields
                            .into_iter()
                            .map(|(name, ty)| LayoutFieldDef {
                                name,
                                ty,
                                is_weak: false,
                                is_unowned: false,
                            })
                            .collect(),
                    })
                    .collect(),
                destructor: None,
            });
        }
        LayoutTable::build(
            self.target_layout,
            &self.type_ctx.interner,
            struct_defs,
            union_defs,
        )
    }

    /// Maps every tagged nominal type to its source-level display name (`Map<string, object>`),
    /// which `typeof` reports at runtime. [`dream_hir::TypeLayout::name`] cannot serve: it is the
    /// C-safe mangled spelling used to name generated symbols.
    pub(in crate::analyzer) fn hir_build_type_names(
        &mut self,
        layouts: &dream_hir::LayoutTable,
    ) -> dream_hir::TypeNameTable {
        let tagged: Vec<TypeId> = layouts
            .structs
            .keys()
            .chain(layouts.unions.keys())
            .copied()
            .collect();
        tagged
            .into_iter()
            .map(|ty| {
                let name =
                    dream_types::display_name(&self.type_ctx.interner, &self.type_ctx.defs, ty);
                (ty, name)
            })
            .collect()
    }

    /// Collects the module's host/interop imports: every non-intrinsic `extern fun` (top-level or a
    /// class/`extend` static member) becomes an [`HImport`] the backend emits as `(import ...)`.
    /// Overloaded externs share one imported field, so entries are de-duplicated by name.
    pub(in crate::analyzer) fn hir_build_imports(
        &mut self,
        node: &crate::module_graph::ProgramView,
    ) -> Vec<HImport> {
        use dream_types::method_fn;
        let mut imports: Vec<HImport> = Vec::new();
        // Each candidate is paired with the name it was *registered* under: top-level externs keep
        // their bare name, while class/`extend` static externs are mangled `{Type}_{method}` (the
        // name the call site resolves to). Using the bare method name for a class extern would fail
        // the def lookup and silently drop the import (its call site then falls back to `$def{N}`).
        let graph = self.graph;
        let top = node.functions.iter().map(|f| {
            (
                *f,
                f.name.text.clone(),
                graph.module_for_file(f.file_path.as_deref()),
            )
        });
        let class_methods = node.structs.iter().flat_map(|s| {
            s.methods.iter().map(move |m| {
                (
                    m,
                    method_fn(&s.name.text, &m.name.text),
                    graph.module_for_file(s.file_path.as_deref()),
                )
            })
        });
        let extend_methods = node.extends.iter().flat_map(|e| {
            e.methods.iter().map(move |m| {
                (
                    m,
                    method_fn(&e.target.text, &m.name.text),
                    graph.module_for_file(e.file_path.as_deref()),
                )
            })
        });
        // A generic class's `extern`/`@intrinsic` methods (e.g. `Cell<T>.raw_host`) are
        // duplicated per instantiation under a mangled `{Type_args}_{method}` name absent from
        // `node.structs` (which only ever holds the unmangled template) — walk
        // `generic_struct_instances` (recorded by `ensure_struct_instantiated`) for those instead.
        let generic_instance_methods: Vec<(
            &dream_syntax::nodes::FunctionNode,
            String,
            dream_types::ModuleId,
        )> = self
            .generic_struct_instances
            .iter()
            .filter_map(|(base, args)| {
                let template = *self.generic_struct(base.as_str())?;
                let mangled = dream_syntax::nodes::types::mangle_generic(base, args);
                Some(
                    template
                        .methods
                        .iter()
                        .map(move |m| {
                            (
                                m,
                                method_fn(&mangled, &m.name.text),
                                graph.module_for_file(template.file_path.as_deref()),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect();
        for (func, sym_name, owner) in top
            .chain(class_methods)
            .chain(extend_methods)
            .chain(generic_instance_methods)
        {
            if !func.is_extern || dream_abi::intrinsics::has_intrinsic_attr(&func.attributes) {
                continue;
            }
            self.type_ctx.set_scope(owner);
            let sym_name = self
                .function_table
                .resolve_item_namespace(&graph.modules[owner.0 as usize].path, &sym_name)
                .unwrap_or(sym_name);
            // Match the def the call site resolves to, so the emitter's symbol table maps the call
            // onto this import's `$name`. Unregistered externs (should not happen) are skipped.
            let Some(def) = self.type_ctx.resolve(DefKind::Function, &sym_name) else {
                continue;
            };
            if imports.iter().any(|import| import.def == def) {
                continue;
            }
            let (module, field) = extern_import_target(func);
            let param_by_ref: Vec<bool> = func.parameters.iter().map(|p| p.is_ref).collect();
            let c_wide_strings =
                dream_abi::attributes::c_marshal_charset(&func.attributes) == Some("lpwstr");
            let async_host = dream_abi::attributes::has_async_host_attr(&func.attributes);
            let params = func
                .parameters
                .iter()
                .map(|p| self.type_ctx.lower(&p.type_))
                .collect();
            // Async host imports always return a `Future` handle (`i32`), including `async …: void`.
            // Sync imports omit a result only for void.
            let ret = if func.is_async {
                let base = func.return_type.clone().unwrap_or(Type::Void);
                Some(self.type_ctx.lower(&Self::future_type(base)))
            } else {
                match func.return_type.as_ref() {
                    Some(t) if *t != Type::Void => Some(self.type_ctx.lower(t)),
                    _ => None,
                }
            };
            let (c_params, c_ret) = if dream_abi::attributes::has_c_attr(&func.attributes) {
                self.c_shapes(func)
            } else {
                (Vec::new(), dream_hir::CShape::Void)
            };
            imports.push(HImport {
                def,
                name: sym_name,
                module,
                field,
                params,
                param_by_ref,
                ret,
                is_async: func.is_async,
                async_host,
                c_wide_strings,
                c_params,
                c_ret,
                c_stdcall: dream_abi::attributes::c_call_conv(&func.attributes)
                    == dream_abi::attributes::CCallConv::Stdcall,
            });
        }
        imports
    }

    /// Collects every `@intrinsic("key")` extern as `(callee DefId, key)`. Unlike host imports these
    /// have no `(import ...)` and no emitted body: their call sites resolve directly to the runtime
    /// helper `$<key>` (`string_alloc`, `char_at`, …) or, for `sleep`, are recognized as an async
    /// intrinsic. Methods are looked up under their mangled `{Type}_{method}` def name (the name the
    /// call site resolves to), matching how they were registered.
    pub(in crate::analyzer) fn hir_build_intrinsics(
        &mut self,
        node: &crate::module_graph::ProgramView,
    ) -> Vec<(dream_types::DefId, String)> {
        use dream_types::method_fn;
        let mut out: Vec<(dream_types::DefId, String)> = self.intrinsic_defs.clone();
        for func in node.functions.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(func.file_path.as_deref()));
            if let Some(key) = dream_abi::intrinsics::intrinsic_key(&func.attributes) {
                if let Some(def) = self.type_ctx.resolve(DefKind::Function, &func.name.text) {
                    out.push((def, key));
                }
            }
        }
        for s in node.structs.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(s.file_path.as_deref()));
            for m in s.methods.iter() {
                if let Some(key) = dream_abi::intrinsics::intrinsic_key(&m.attributes) {
                    let mangled = method_fn(&s.name.text, &m.name.text);
                    if let Some(def) = self.type_ctx.resolve(DefKind::Function, &mangled) {
                        out.push((def, key));
                    }
                }
            }
        }
        for e in node.extends.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(e.file_path.as_deref()));
            for m in e.methods.iter() {
                if let Some(key) = dream_abi::intrinsics::intrinsic_key(&m.attributes) {
                    let mangled = method_fn(&e.target.text, &m.name.text);
                    if let Some(def) = self.type_ctx.resolve(DefKind::Function, &mangled) {
                        out.push((def, key));
                    }
                }
            }
        }
        // See the matching comment in `hir_build_imports`: a generic class's `@intrinsic` methods
        // need one binding per monomorphization, under its mangled name, not the template's.
        for (base, args) in self.generic_struct_instances.iter() {
            let Some(template) = self.generic_struct(base.as_str()) else {
                continue;
            };
            let mangled_struct = dream_syntax::nodes::types::mangle_generic(base, args);
            for m in template.methods.iter() {
                if let Some(key) = dream_abi::intrinsics::intrinsic_key(&m.attributes) {
                    let mangled = method_fn(&mangled_struct, &m.name.text);
                    if let Some(def) = self.type_ctx.resolve(DefKind::Function, &mangled) {
                        out.push((def, key));
                    }
                }
            }
        }
        out
    }
}
