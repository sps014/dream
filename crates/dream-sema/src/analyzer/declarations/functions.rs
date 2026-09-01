//! Function-signature registration (including overload/`main` validation and public-visibility
//! leakage checks) and the body-analysis / pending-instantiation fixpoint passes.

use super::*;
use crate::function_table::FunctionTableInfo;
use dream_syntax::nodes::types::strip_array;
use dream_syntax::nodes::Type;

impl<'a> Analyzer<'a> {
    /// Pass 1: register every (non-generic) function signature; stash generic templates.
    pub(in crate::analyzer) fn register_functions(
        &mut self,
        node: &'a ProgramNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for function in node.functions.iter() {
            diagnostics.file_path = file_path_string(&function.file_path);
            self.check_reserved_name(&function.name, "function", diagnostics);
            if function.generic_parameters.is_some() {
                if dream_abi::attributes::has_test_attr(&function.attributes) {
                    diagnostics.report_error(
                        format!("@test function '{}' cannot be generic", function.name.text),
                        Some(function.name.position),
                    );
                }
                if dream_abi::attributes::is_gpu_shader_attr(&function.attributes)
                    || dream_abi::attributes::has_gpu_helper_attr(&function.attributes)
                {
                    let kind = if dream_abi::attributes::has_compute_attr(&function.attributes) {
                        "@compute"
                    } else if dream_abi::attributes::has_vertex_attr(&function.attributes) {
                        "@vertex"
                    } else if dream_abi::attributes::has_fragment_attr(&function.attributes) {
                        "@fragment"
                    } else {
                        "@gpu"
                    };
                    diagnostics.report_error(
                        format!("{kind} function '{}' cannot be generic", function.name.text),
                        Some(function.name.position),
                    );
                }
                self.type_ctx.register(
                    DefKind::Function,
                    &function.name.text,
                    generic_param_names(&function.generic_parameters),
                );
                self.generic_functions
                    .insert(function.name.text.clone(), function);
                continue;
            }
            if function.visibility.is_public() {
                self.check_public_visibility(function, diagnostics);
            }
            let mut info = FunctionTableInfo::from(function);
            info.declaring_module = self.module_of(function.file_path.as_ref());
            if let Some(ret) = &function.return_type {
                self.check_type_not_static_class(ret, diagnostics);
            }
            for p in &function.parameters {
                self.check_type_not_static_class(&p.type_, diagnostics);
            }
            if dream_abi::attributes::has_test_attr(&function.attributes) {
                self.validate_test_function(function, diagnostics);
            }
            if function.is_extern && dream_abi::attributes::has_c_attr(&function.attributes) {
                self.validate_c_extern_signature(function, diagnostics);
            }
            if info.is_compute {
                self.validate_compute_shader(function, &info, diagnostics);
            }
            if info.is_vertex {
                self.validate_vertex_shader(function, diagnostics);
            }
            if info.is_fragment {
                self.validate_fragment_shader(function, diagnostics);
            }
            if let Err(e) =
                self.function_table
                    .add_overload(&function.name.text, info, &mut self.type_ctx)
            {
                diagnostics.report_error(e.to_string(), Some(function.name.position));
            }
        }
        // Register a distinct `DefId` for every non-generic function under its *emitted* name (the
        // bare base when unique, the signature-mangled key when overloaded). Deferred to here so the
        // full overload set is known: overloaded declarations must not collide on a single base def.
        for function in node.functions.iter() {
            if function.generic_parameters.is_some() {
                continue;
            }
            let param_types: Vec<Type> = function
                .parameters
                .iter()
                .map(|p| p.type_.clone())
                .collect();
            let module = self.module_of(function.file_path.as_ref());
            let emitted = self.function_table.resolve_emitted_name_scoped(
                &function.name.text,
                module.as_ref(),
                &param_types,
                &mut self.type_ctx,
            );
            self.type_ctx.register(DefKind::Function, &emitted, vec![]);
        }
        // The entry point is exported under the fixed name `main`. It may be declared as `main()`
        // or `main(args: string[])`, but not overloaded or given any other signature.
        // Library crates reject a top-level `main` in the primary compilation file.
        if self.crate_type == CrateType::Lib {
            if let Ok(info) = self.function_table.get_function(&"main".to_string()) {
                let in_primary = match (&info.declaring_file, &self.primary_file) {
                    (Some(decl), Some(primary)) => paths_equal(decl.as_ref(), primary),
                    _ => true,
                };
                if in_primary {
                    diagnostics.report_error(
                        "library crates must not declare a top-level 'main' \
                         (use --crate-type bin for runnable programs)"
                            .to_string(),
                        None,
                    );
                }
            }
        } else if self.function_table.is_overloaded("main") {
            diagnostics.report_error("'main' cannot be overloaded".to_string(), None);
        } else if let Ok(info) = self.function_table.get_function(&"main".to_string()) {
            let ok = info.parameters.is_empty()
                || (info.parameters.len() == 1 && info.parameters[0] == "string[]");
            if !ok {
                diagnostics.report_error(
                    "'main' must be declared as 'main()' or 'main(args: string[])'".to_string(),
                    None,
                );
            }
        }
    }

    /// Ensures a `public` function does not leak a private (non-`public`) type through its
    /// signature, including nested generics (`List<Private>`, `Option<Private>`), tuples, arrays,
    /// function types, and private enums/unions/interfaces.
    pub(in crate::analyzer) fn check_public_visibility(
        &self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let signature_types = function
            .return_type
            .iter()
            .chain(function.parameters.iter().map(|p| &p.type_));
        for type_to_check in signature_types {
            self.check_public_type_exposed(type_to_check, function, diagnostics);
        }
    }

    fn check_public_type_exposed(
        &self,
        ty: &Type,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        match ty {
            Type::Array(inner) => self.check_public_type_exposed(inner, function, diagnostics),
            Type::Tuple(elems) => {
                for e in elems {
                    self.check_public_type_exposed(e, function, diagnostics);
                }
            }
            Type::Function(params, ret) => {
                for p in params {
                    self.check_public_type_exposed(p, function, diagnostics);
                }
                self.check_public_type_exposed(ret, function, diagnostics);
            }
            Type::Struct(token, args) => {
                self.check_nominal_not_private(&token.text, function, diagnostics);
                if let Some(args) = args {
                    for a in args {
                        self.check_public_type_exposed(a, function, diagnostics);
                    }
                }
            }
            _ => {}
        }
    }

    fn check_nominal_not_private(
        &self,
        name: &str,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if let Some(struct_info) = self.struct_table.get_struct(name) {
            if !struct_info.visibility.is_public() {
                diagnostics.report_error(
                    format!(
                        "Public function '{}' exposes private class '{}'",
                        function.name.text, name
                    ),
                    Some(function.name.position),
                );
            }
        }
        if let Some((_, visibility)) = self.type_visibility.get(name) {
            if !visibility.is_public() {
                diagnostics.report_error(
                    format!(
                        "Public function '{}' exposes private type '{}'",
                        function.name.text, name
                    ),
                    Some(function.name.position),
                );
            }
        }
    }

    /// Pass 2: analyze the body of every concrete function.
    pub(in crate::analyzer) fn analyze_function_bodies(
        &mut self,
        node: &'a ProgramNode<'a>,
        symbol_table_map: &mut HashMap<String, Rc<RefCell<SymbolTable>>>,
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
            diagnostics.file_path = file_path_string(&function.file_path);
            let table = self.analyze_function(function, diagnostics)?;
            // Key the symbol table by the emitted name so overloaded functions (which share a
            // base name but emit distinct mangled names) each get their own entry, matching the
            // name codegen uses.
            let param_types: Vec<Type> = function
                .parameters
                .iter()
                .map(|p| p.type_.clone())
                .collect();
            let module = self.module_of(function.file_path.as_ref());
            let key = self.function_table.resolve_emitted_name_scoped(
                &function.name.text,
                module.as_ref(),
                &param_types,
                &mut self.type_ctx,
            );
            symbol_table_map.insert(key, table);
        }
        Ok(())
    }

    /// Passes 3 & 4 (combined fixpoint): analyze the bodies of every monomorphized generic
    /// function instance and every (de-sugared) struct method.
    ///
    /// Analyzing one body can lazily instantiate *more* generics — a struct method that uses
    /// `List<JsonValue>` queues new struct methods, and a builder that calls `List<JsonValue>()`
    /// queues a new generic function instance. The two feed each other, so we loop until neither
    /// the generic-function set nor the struct-method list grows. Both instantiation paths are
    /// idempotent (guarded by the struct/function tables), so this terminates.
    pub(in crate::analyzer) fn analyze_pending_instantiations(
        &mut self,
        symbol_table_map: &mut HashMap<String, Rc<RefCell<SymbolTable>>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let mut processed_generics: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut method_index = 0;
        // A generic whose field types amplify under substitution (e.g. a `List<fun(T): bool>`
        // field on `class C<T>` combined with something returning `C<fun(T): bool>`) expands
        // without bound, and each expansion round makes the mangled monomorphized name strictly
        // longer. Healthy mangled names stay well under a few hundred characters, so watching
        // for runaway name growth turns an infinite loop into an immediate diagnostic.
        let mut max_mangled_len: usize = 0;
        let mut items_processed: usize = 0;
        loop {
            let mut progressed = false;

            // Monomorphized generic function instances (e.g. `List<JsonValue>`, `swap_int_string`).
            let pending: Vec<String> = self
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
                let table = self.with_generic_bindings(bindings, |s| {
                    s.analyze_function(template, diagnostics)
                })?;
                progressed = true;
                items_processed += 1;
                check_instantiation_bounds(
                    &mut max_mangled_len,
                    &mut items_processed,
                    &mangled_name,
                    diagnostics,
                )?;
                symbol_table_map.insert(mangled_name, table);
            }

            // Arrow-lambdas lowered to synthesized top-level functions (see `expressions::lambda`).
            // The lambda literal itself is never generic in v1, but the *enclosing* method it was
            // written in might be (e.g. a lambda inside a `WebWorker<TIn, TOut>` method) - re-apply
            // the bindings captured at its use site so its body sees the same substitution.
            let pending_lambdas: Vec<String> = self
                .pending_lambdas
                .keys()
                .filter(|k| !processed_generics.contains(*k))
                .cloned()
                .collect();
            for name in pending_lambdas {
                processed_generics.insert(name.clone());
                items_processed += 1;
                check_instantiation_bounds(
                    &mut max_mangled_len,
                    &mut items_processed,
                    &name,
                    diagnostics,
                )?;
                let (template, bindings) = match self.pending_lambdas.get(&name) {
                    Some((t, b)) => (*t, b.clone()),
                    None => continue,
                };
                diagnostics.file_path = file_path_string(&template.file_path);
                let table = self.with_generic_bindings(bindings, |s| {
                    s.analyze_function(template, diagnostics)
                })?;
                symbol_table_map.insert(name, table);
                progressed = true;
            }

            // De-sugared struct methods, including those for newly instantiated generic structs.
            while method_index < self.struct_methods.len() {
                let (method, bindings) = self.struct_methods[method_index].clone();
                method_index += 1;
                diagnostics.file_path = file_path_string(&method.file_path);
                let table = self
                    .with_generic_bindings(bindings, |s| s.analyze_function(method, diagnostics))?;
                // Key by the emitted name so overloaded methods each get a distinct entry (the
                // parameter list includes the implicit `this`).
                let param_types: Vec<Type> =
                    method.parameters.iter().map(|p| p.type_.clone()).collect();
                let key = self.function_table.resolve_emitted_name(
                    &method.name.text,
                    &param_types,
                    &mut self.type_ctx,
                );
                items_processed += 1;
                check_instantiation_bounds(
                    &mut max_mangled_len,
                    &mut items_processed,
                    &key,
                    diagnostics,
                )?;
                symbol_table_map.insert(key, table);
                progressed = true;
            }

            if !progressed {
                break;
            }
        }
        Ok(())
    }

    fn validate_test_function(&self, function: &FunctionNode<'a>, diagnostics: &mut DiagnosticBag) {
        if function.name.text == "main" {
            diagnostics.report_error(
                "'main' cannot be marked @test".to_string(),
                Some(function.name.position),
            );
        }
        if function.is_async {
            diagnostics.report_error(
                format!("@test function '{}' cannot be async", function.name.text),
                Some(function.name.position),
            );
        }
        if function.is_extern {
            diagnostics.report_error(
                format!("@test function '{}' cannot be extern", function.name.text),
                Some(function.name.position),
            );
        }
        if !function.parameters.is_empty() {
            diagnostics.report_error(
                format!(
                    "@test function '{}' must take no parameters",
                    function.name.text
                ),
                Some(function.name.position),
            );
        }
        let ret = function
            .return_type
            .as_ref()
            .map(|t| t.get_type())
            .unwrap_or_else(|| "void".to_string());
        if ret != "void" {
            diagnostics.report_error(
                format!("@test function '{}' must return void", function.name.text),
                Some(function.name.position),
            );
        }
        if dream_abi::attributes::is_gpu_shader_attr(&function.attributes)
            || dream_abi::attributes::has_gpu_helper_attr(&function.attributes)
            || dream_abi::attributes::has_generator_attr(&function.attributes)
        {
            diagnostics.report_error(
                format!(
                    "@test function '{}' cannot also be a generator or GPU shader",
                    function.name.text
                ),
                Some(function.name.position),
            );
        }
    }

    /// Rejects `@c` extern signatures whose nominal struct parameter types are not `@unmanaged`
    /// (heap classes or unions carry ARC headers and non-trivial layouts, neither of which the C
    /// FFI trampoline can pass by pointer safely). Primitive/scalar params, `string`, `byte[]`,
    /// and `ref` out-params are always accepted; a struct that satisfies `Unmanaged` (a value
    /// struct with only value-typed / primitive fields) is accepted too.
    fn validate_c_extern_signature(
        &mut self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        use dream_syntax::nodes::ConstraintKind;
        for param in &function.parameters {
            let base = param.type_.get_type();
            let base = strip_array(&base);
            // Only nominal names (not primitives / builtins / arrays / functions) are candidates
            // for the unmanaged check: those the analyzer already knows about are the ones a `@c`
            // callee would treat as a raw struct pointer.
            if !self.name_is_known_nominal(base) {
                continue;
            }
            if self.type_satisfies_kind(&param.type_, ConstraintKind::Unmanaged) {
                continue;
            }
            diagnostics.report_error(
                format!(
                    "'@c' extern '{}' parameter '{}' has type '{}', which is not unmanaged; C ABI parameters must be primitives, `string`, `byte[]`, or an @unmanaged value struct",
                    function.name.text,
                    param.name.text,
                    self.ty_display(&param.type_)
                ),
                Some(param.name.position),
            );
        }
    }

    /// True when `name` is a class/struct/union the analyzer has registered — cheap gate to skip
    /// primitives/builtins/arrays before running the more expensive `type_satisfies_kind` check.
    fn name_is_known_nominal(&self, name: &str) -> bool {
        self.struct_table.get_struct(name).is_some() || self.union_table.contains_key(name)
    }

    fn validate_compute_shader(
        &mut self,
        function: &FunctionNode<'a>,
        info: &FunctionTableInfo,
        diagnostics: &mut DiagnosticBag,
    ) {
        if function.is_async {
            diagnostics.report_error(
                format!("@compute kernel '{}' cannot be async", function.name.text),
                Some(function.name.position),
            );
        }
        if function.is_extern {
            diagnostics.report_error(
                format!("@compute kernel '{}' cannot be extern", function.name.text),
                Some(function.name.position),
            );
        }
        if !matches!(info.return_type, None | Some(Type::Void)) {
            diagnostics.report_error(
                format!("@compute kernel '{}' must return void", function.name.text),
                Some(function.name.position),
            );
        }
        for p in function.parameters.iter() {
            if !is_compute_param_type(&p.type_) {
                diagnostics.report_error(
                    format!(
                        "@compute kernel '{}' parameter '{}' has type '{}'; only primitives, unmanaged value structs, GpuBuffer<T>, GpuTexture, and GpuSampler are allowed",
                        function.name.text,
                        p.name.text,
                        self.ty_display(&p.type_)
                    ),
                    Some(p.name.position),
                );
            }
        }
    }

    fn validate_vertex_shader(
        &mut self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if function.is_async {
            diagnostics.report_error(
                format!("@vertex shader '{}' cannot be async", function.name.text),
                Some(function.name.position),
            );
        }
        if function.is_extern {
            diagnostics.report_error(
                format!("@vertex shader '{}' cannot be extern", function.name.text),
                Some(function.name.position),
            );
        }
        match &function.return_type {
            Some(Type::Struct(tok, None)) => {
                if let Some(info) = self.struct_table.get_struct(&tok.text) {
                    let ok = info.fields.iter().any(|(fname, f)| {
                        let is_pos =
                            f.builtin.as_deref() == Some("position") || fname == "position";
                        is_pos && matches!(&f.type_, Type::Struct(t, None) if t.text == "GpuVec4")
                    });
                    if !ok {
                        diagnostics.report_error(
                            format!(
                                "@vertex shader '{}' return struct '{}' must have a position builtin (`position: GpuVec4` or `@builtin(\"position\") GpuVec4`)",
                                function.name.text, tok.text
                            ),
                            Some(function.name.position),
                        );
                    }
                    self.check_location_duplicates(info, diagnostics, function.name.position);
                } else {
                    diagnostics.report_error(
                        format!(
                            "@vertex shader '{}' return type '{}' is not a known struct",
                            function.name.text, tok.text
                        ),
                        Some(function.name.position),
                    );
                }
            }
            _ => {
                diagnostics.report_error(
                    format!(
                        "@vertex shader '{}' must return a value struct with a position builtin",
                        function.name.text
                    ),
                    Some(function.name.position),
                );
            }
        }
        for p in function.parameters.iter() {
            if !is_render_param_type(&p.type_) {
                diagnostics.report_error(
                    format!(
                        "@vertex shader '{}' parameter '{}' has type '{}'; only primitives, unmanaged value structs, @readonly GpuBuffer, GpuTexture, and GpuSampler are allowed",
                        function.name.text,
                        p.name.text,
                        self.ty_display(&p.type_)
                    ),
                    Some(p.name.position),
                );
            }
            if let Type::Struct(tok, None) = &p.type_ {
                if !matches!(tok.text.as_str(), "GpuTexture" | "GpuSampler") {
                    if let Some(info) = self.struct_table.get_struct(&tok.text) {
                        self.check_location_duplicates(info, diagnostics, p.name.position);
                    }
                }
            }
        }
    }

    fn validate_fragment_shader(
        &mut self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if function.is_async {
            diagnostics.report_error(
                format!("@fragment shader '{}' cannot be async", function.name.text),
                Some(function.name.position),
            );
        }
        if function.is_extern {
            diagnostics.report_error(
                format!("@fragment shader '{}' cannot be extern", function.name.text),
                Some(function.name.position),
            );
        }
        match &function.return_type {
            Some(Type::Struct(tok, None)) if tok.text == "GpuVec4" => {}
            Some(Type::Struct(tok, None)) => {
                if let Some(info) = self.struct_table.get_struct(&tok.text) {
                    let mut has_color = false;
                    for (fname, field) in &info.fields {
                        if let Some(b) = field.builtin.as_deref() {
                            if b != "frag_depth" {
                                diagnostics.report_error(
                                    format!(
                                        "@fragment shader '{}' output field '{}' has unsupported @builtin(\"{b}\")",
                                        function.name.text, fname
                                    ),
                                    Some(function.name.position),
                                );
                            } else if !matches!(&field.type_, Type::Float(_)) {
                                diagnostics.report_error(
                                    format!(
                                        "@fragment shader '{}' frag_depth field '{}' must be float",
                                        function.name.text, fname
                                    ),
                                    Some(function.name.position),
                                );
                            }
                        } else {
                            has_color = true;
                            if !matches!(&field.type_, Type::Struct(t, None) if t.text == "GpuVec4")
                            {
                                diagnostics.report_error(
                                    format!(
                                        "@fragment shader '{}' color output '{}' must be GpuVec4",
                                        function.name.text, fname
                                    ),
                                    Some(function.name.position),
                                );
                            }
                        }
                    }
                    if !has_color {
                        diagnostics.report_error(
                            format!(
                                "@fragment shader '{}' output struct '{}' needs at least one color @location field",
                                function.name.text, tok.text
                            ),
                            Some(function.name.position),
                        );
                    }
                    self.check_location_duplicates(info, diagnostics, function.name.position);
                } else {
                    diagnostics.report_error(
                        format!(
                            "@fragment shader '{}' return type '{}' is not a known struct",
                            function.name.text, tok.text
                        ),
                        Some(function.name.position),
                    );
                }
            }
            _ => {
                diagnostics.report_error(
                    format!(
                        "@fragment shader '{}' must return GpuVec4 or an unmanaged output struct",
                        function.name.text
                    ),
                    Some(function.name.position),
                );
            }
        }
        if let Some(first) = function.parameters.first() {
            if let Type::Struct(tok, None) = &first.type_ {
                if !matches!(tok.text.as_str(), "GpuTexture" | "GpuSampler") {
                    if let Some(info) = self.struct_table.get_struct(&tok.text) {
                        let ok = info.fields.iter().any(|(fname, f)| {
                            let is_pos =
                                f.builtin.as_deref() == Some("position") || fname == "position";
                            is_pos
                                && matches!(&f.type_, Type::Struct(t, None) if t.text == "GpuVec4")
                        });
                        if !ok {
                            diagnostics.report_error(
                                format!(
                                    "@fragment shader '{}' input struct '{}' must include a position builtin",
                                    function.name.text, tok.text
                                ),
                                Some(first.name.position),
                            );
                        }
                        self.check_location_duplicates(info, diagnostics, first.name.position);
                    }
                }
            }
        }
        for p in function.parameters.iter() {
            if !is_render_param_type(&p.type_) {
                diagnostics.report_error(
                    format!(
                        "@fragment shader '{}' parameter '{}' has type '{}'; only primitives, unmanaged value structs, @readonly GpuBuffer, GpuTexture, and GpuSampler are allowed",
                        function.name.text,
                        p.name.text,
                        self.ty_display(&p.type_)
                    ),
                    Some(p.name.position),
                );
            }
        }
    }

    fn check_location_duplicates(
        &self,
        info: &crate::struct_table::StructInfo,
        diagnostics: &mut DiagnosticBag,
        span: dream_text::text_span::TextSpan,
    ) {
        let mut used = indexmap::IndexMap::<u32, String>::new();
        let mut auto = 0u32;
        for (fname, field) in &info.fields {
            if field.builtin.is_some() || fname == "position" {
                continue;
            }
            let loc = match field.location {
                Some(n) => n,
                None => {
                    while used.contains_key(&auto) {
                        auto += 1;
                    }
                    auto
                }
            };
            if let Some(prev) = used.insert(loc, fname.clone()) {
                diagnostics.report_error(
                    format!(
                        "duplicate @location({}) on fields '{}' and '{}' in struct '{}'",
                        loc, prev, fname, info.name
                    ),
                    Some(span),
                );
            }
            if field.location.is_none() {
                auto = loc + 1;
            }
        }
    }
}

/// Element type of `GpuBuffer<T>`, if `ty` is that form.
pub(crate) fn gpu_buffer_elem_type(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Struct(tok, Some(args)) if tok.text == "GpuBuffer" && args.len() == 1 => {
            Some(&args[0])
        }
        _ => None,
    }
}

/// Kernel parameters: scalars, unmanaged value structs, `GpuBuffer<T>`, `GpuTexture`, `GpuSampler`.
fn is_compute_param_type(ty: &Type) -> bool {
    match ty {
        Type::Integer(_)
        | Type::Float(_)
        | Type::Boolean(_)
        | Type::Byte(_)
        | Type::UInt(_)
        | Type::Long(_)
        | Type::ULong(_) => true,
        Type::Struct(tok, Some(args)) if tok.text == "GpuBuffer" && args.len() == 1 => {
            is_compute_elem_type(&args[0])
        }
        Type::Struct(tok, None) => {
            if matches!(tok.text.as_str(), "GpuTexture" | "GpuSampler") {
                return true;
            }
            // Allow unmanaged value structs by name; GpuId3 is synthetic. Reject known heap types.
            !matches!(
                tok.text.as_str(),
                "string" | "List" | "Map" | "Set" | "object" | "js"
            )
        }
        Type::String(_) | Type::Object(_) | Type::Char(_) | Type::Array(_) => false,
        _ => false,
    }
}

/// Vertex/fragment parameters: primitives, unmanaged value structs, textures/samplers,
/// and read-only `GpuBuffer<T>` (storage).
fn is_render_param_type(ty: &Type) -> bool {
    match ty {
        Type::Integer(_)
        | Type::Float(_)
        | Type::Boolean(_)
        | Type::Byte(_)
        | Type::UInt(_)
        | Type::Long(_)
        | Type::ULong(_) => true,
        Type::Struct(tok, Some(args)) if tok.text == "GpuBuffer" && args.len() == 1 => {
            is_compute_elem_type(&args[0])
        }
        Type::Struct(tok, None) => {
            if matches!(tok.text.as_str(), "GpuTexture" | "GpuSampler") {
                return true;
            }
            !matches!(
                tok.text.as_str(),
                "string" | "List" | "Map" | "Set" | "object" | "js"
            )
        }
        Type::String(_) | Type::Object(_) | Type::Char(_) | Type::Array(_) => false,
        _ => false,
    }
}

fn is_compute_elem_type(ty: &Type) -> bool {
    match ty {
        Type::Integer(_)
        | Type::Float(_)
        | Type::Boolean(_)
        | Type::Byte(_)
        | Type::UInt(_)
        | Type::Long(_)
        | Type::ULong(_) => true,
        Type::Struct(tok, None) => !matches!(
            tok.text.as_str(),
            "string" | "List" | "Map" | "Set" | "object" | "js"
        ),
        _ => false,
    }
}

/// Healthy programs monomorphize a few thousand methods with short mangled names. A generic
/// whose field types amplify under substitution (e.g. a `List<fun(T): bool>` field on
/// `class C<T>` while something derives `C<fun(T): bool>`) diverges: every expansion round
/// wraps the type again, so mangled names grow without limit while breadth explodes. Either
/// signature trips these bounds and turns an infinite loop into an immediate diagnostic.
const MAX_MANGLED_NAME_LEN: usize = 512;
const MAX_INSTANTIATION_ITEMS: usize = 50_000;

fn check_instantiation_bounds(
    max_len: &mut usize,
    items: &mut usize,
    name: &str,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), SemanticError> {
    let diverging = *items > MAX_INSTANTIATION_ITEMS;
    if name.len() > *max_len {
        *max_len = name.len();
    }
    if !diverging && *max_len <= MAX_MANGLED_NAME_LEN {
        return Ok(());
    }
    diagnostics.report_error(
        format!(
            "generic instantiation grew too deep while expanding '{}...': monomorphization is diverging, usually because a generic class has a field that wraps its own type parameter (e.g. `ps: List<fun(T): bool>` on `class C<T>`) while something also derives `C<fun(T): bool>` from it; restructure the class so its field types do not nest the parameter deeper",
            &name[..name.len().min(120)]
        ),
        None,
    );
    Err(SemanticError::AnalysisFailed)
}
