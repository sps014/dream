use super::*;

impl Builder {
    pub(crate) fn resolve(&self, name: &str, scope: usize, before: usize) -> Option<&Decl> {
        let local = self
            .decls
            .iter()
            .filter(|d| {
                d.name == name
                    && d.scope == scope
                    && matches!(d.kind, SymKind::Variable | SymKind::Param)
                    && d.start <= before
            })
            .max_by_key(|d| d.start);
        if local.is_some() {
            return local;
        }
        // File-scope fallback: free functions, types, and top-level `let`/`const` globals (which
        // carry `scope == GLOBAL` and `SymKind::Variable`).
        self.decls.iter().find(|d| {
            d.name == name
                && d.scope == GLOBAL
                && matches!(
                    d.kind,
                    SymKind::Function
                        | SymKind::Class
                        | SymKind::Struct
                        | SymKind::Interface
                        | SymKind::Enum
                        | SymKind::Variable
                )
        })
    }

    pub(crate) fn set_current_file(&mut self, path: Option<&str>) {
        self.current_file = if self.is_main {
            None
        } else {
            path.map(str::to_string)
        };
    }

    pub(crate) fn walk_program_for_imports(&mut self, program: &ProgramNode) {
        {
            let mut ctx = self.type_ctx.borrow_mut();
            ctx.register(
                DefKind::Struct,
                dream::syntax::nodes::types::FUTURE_TYPE,
                vec!["T".into()],
            );
            for st in &program.structs {
                ctx.register(
                    DefKind::Struct,
                    &st.name.text,
                    param_names_from_tokens(&st.generic_parameters),
                );
            }
            for en in &program.enums {
                ctx.register(
                    if en.is_data_enum() {
                        DefKind::Union
                    } else {
                        DefKind::Enum
                    },
                    &en.name.text,
                    param_names_from_tokens(&en.generic_parameters),
                );
            }
            for iface in &program.interfaces {
                ctx.register(
                    DefKind::Interface,
                    &iface.name.text,
                    param_names_from_tokens(&iface.generic_parameters),
                );
            }
        }
        for func in &program.functions {
            self.set_current_file(func.file_path.as_deref());
            let detail = signature(func);
            self.push_decl(&func.name, SymKind::Function, detail, GLOBAL, None);
            self.record_callable(func, None);
            self.fn_params
                .insert(func.name.text.clone(), param_names(func));
        }
        for st in &program.structs {
            self.set_current_file(st.file_path.as_deref());
            let (kind, keyword) = if st.is_value {
                (
                    SymKind::Struct,
                    if st.is_ref_struct {
                        "ref struct"
                    } else {
                        "struct"
                    },
                )
            } else {
                (SymKind::Class, "class")
            };
            let generics = st
                .generic_parameters
                .as_ref()
                .map(|params| {
                    let names = params
                        .iter()
                        .map(|p| p.text.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("<{names}>")
                })
                .unwrap_or_default();
            let detail = format!("{} {}{}", keyword, st.name.text, generics);
            self.push_decl(&st.name, kind, detail, GLOBAL, None);
            for field in &st.fields {
                let field_ty = field.field_type.display_name();
                let detail = format!("{}.{}: {}", st.name.text, field.name.text, field_ty);
                self.push_decl(&field.name, SymKind::Field, detail, GLOBAL, Some(field_ty));
                self.record_decl_type(&field.field_type);
                self.member_owners
                    .insert(self.decls.len() - 1, st.name.text.clone());
            }
            for method in &st.methods {
                let detail = method_detail(&st.name.text, method);
                self.push_decl(&method.name, SymKind::Method, detail, GLOBAL, None);
                self.record_callable(method, Some(&st.name.text));
                if method.name.text == CONSTRUCTOR_NAME {
                    self.ctor_params
                        .insert(st.name.text.clone(), param_names(method));
                }
            }
        }
        for en in &program.enums {
            self.set_current_file(en.file_path.as_deref());
            let generics = en
                .generic_parameters
                .as_ref()
                .map(|params| {
                    let names = params
                        .iter()
                        .map(|p| p.text.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("<{}>", names)
                })
                .unwrap_or_default();
            let detail = format!("enum {}{}", en.name.text, generics);
            self.push_decl(&en.name, SymKind::Enum, detail, GLOBAL, None);
            for variant in &en.variants {
                let detail = if variant.fields.is_empty() {
                    format!("{}.{} = {}", en.name.text, variant.name.text, variant.value)
                } else {
                    let params = variant
                        .fields
                        .iter()
                        .map(|f| format!("{}: {}", f.name.text, f.field_type.display_name()))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{}.{}({})", en.name.text, variant.name.text, params)
                };
                self.push_decl(&variant.name, SymKind::EnumMember, detail, GLOBAL, None);
                for field in &variant.fields {
                    let detail = format!(
                        "{}.{}::{}",
                        en.name.text, variant.name.text, field.name.text
                    );
                    self.push_decl(
                        &field.name,
                        SymKind::Param,
                        detail,
                        GLOBAL,
                        Some(field.field_type.display_name()),
                    );
                }
            }
            for method in &en.methods {
                let detail = method_detail(&en.name.text, method);
                self.push_decl(&method.name, SymKind::Method, detail, GLOBAL, None);
                self.record_callable(method, Some(&en.name.text));
            }
        }
        for iface in &program.interfaces {
            self.set_current_file(iface.file_path.as_deref());
            let generics = iface
                .generic_parameters
                .as_ref()
                .map(|params| {
                    let names = params
                        .iter()
                        .map(|p| p.text.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("<{}>", names)
                })
                .unwrap_or_default();
            let detail = format!("interface {}{}", iface.name.text, generics);
            self.push_decl(&iface.name, SymKind::Interface, detail, GLOBAL, None);
            for method in &iface.methods {
                let detail = method_detail(&iface.name.text, method);
                self.push_decl(&method.name, SymKind::Method, detail, GLOBAL, None);
                self.record_callable(method, Some(&iface.name.text));
            }
        }
        for ext in &program.extends {
            self.set_current_file(ext.file_path.as_deref());
            // Primitive / builtin extend targets (`js`, `object`, `int`, …) have no class/struct
            // declaration of their own — register them as types so `js.` member completion works.
            let target = ext.target.text.as_str();
            if !self.decls.iter().any(|d| {
                d.name == target
                    && matches!(
                        d.kind,
                        SymKind::Class
                            | SymKind::Struct
                            | SymKind::Interface
                            | SymKind::Enum
                            | SymKind::Type
                    )
            }) {
                self.push_decl(
                    &ext.target,
                    SymKind::Type,
                    format!("type {target}"),
                    GLOBAL,
                    None,
                );
            }
            for method in &ext.methods {
                let detail = method_detail(&ext.target.text, method);
                self.push_decl(&method.name, SymKind::Method, detail, GLOBAL, None);
                self.record_callable(method, Some(&ext.target.text));
            }
        }
        // Top-level `let`/`const` variables live at file scope and are visible from every
        // function body, so they are declared here in pass 1 alongside the other globals.
        for global in &program.globals {
            self.set_current_file(global.file_path.as_deref());
            let ty = global
                .declared_type
                .as_ref()
                .map(|t| t.display_name())
                .or_else(|| self.infer_type(&global.initializer, GLOBAL));
            let keyword = if global.is_const { "const" } else { "let" };
            let detail = match &ty {
                Some(t) => format!("{} {}: {}", keyword, global.name.text, t),
                None => format!("{} {}", keyword, global.name.text),
            };
            self.push_decl(&global.name, SymKind::Variable, detail, GLOBAL, ty);
            self.record_binding_type(global.declared_type.as_ref(), &global.initializer, GLOBAL);
        }
        self.current_file = None;
    }

    pub(crate) fn walk_attributes(
        &mut self,
        attributes: &[dream::syntax::nodes::AttributeNode],
        scope: usize,
    ) {
        for attr in attributes {
            // Attribute names are decorators, not types/classes.
            self.add_ref(&attr.name, SymKind::Decorator, scope);
            for arg in &attr.args {
                // Enum-member paths (`HttpMethod.Get`) are identifier refs; other args are literals.
                if let dream::syntax::nodes::AttributeArg::Enum(parts) = arg {
                    for part in parts {
                        self.add_ref(part, SymKind::Variable, scope);
                    }
                }
            }
        }
    }

    pub(crate) fn walk_program(&mut self, program: &ProgramNode) {
        for func in &program.functions {
            self.walk_attributes(&func.attributes, GLOBAL);
            self.walk_function(func, None);
        }
        for st in &program.structs {
            self.walk_attributes(&st.attributes, GLOBAL);
            for field in &st.fields {
                self.walk_attributes(&field.attributes, GLOBAL);
            }
            self.walk_struct(st);
        }
        for en in &program.enums {
            self.walk_attributes(&en.attributes, GLOBAL);
            for variant in &en.variants {
                for field in &variant.fields {
                    self.walk_attributes(&field.attributes, GLOBAL);
                }
            }
            for method in &en.methods {
                self.walk_attributes(&method.attributes, GLOBAL);
                self.walk_method(method, &en.name.text);
            }
        }
        for iface in &program.interfaces {
            self.walk_attributes(&iface.attributes, GLOBAL);
            for method in &iface.methods {
                self.walk_attributes(&method.attributes, GLOBAL);
                // Interface methods have no body; still index their parameter names for hover.
                for param in &method.parameters {
                    let ty = param.type_.display_name();
                    let detail = format!("{}: {}", param.name.text, ty);
                    self.push_decl(&param.name, SymKind::Param, detail, GLOBAL, Some(ty));
                    self.add_type_ref(&param.type_, GLOBAL);
                }
            }
        }
        for ext in &program.extends {
            for method in &ext.methods {
                self.walk_attributes(&method.attributes, GLOBAL);
                self.walk_method(method, &ext.target.text);
            }
        }
        // Walk each top-level initializer at file scope so identifiers inside it become references,
        // and emit a type inlay hint when the variable has no explicit annotation.
        for global in &program.globals {
            if global.declared_type.is_none() {
                if let Some(t) = self.infer_type(&global.initializer, GLOBAL) {
                    self.inlay_hints.push(InlayHintOut {
                        offset: global.name.position.end,
                        label: format!(": {}", t),
                        kind: InlayKind::Type,
                    });
                }
            } else if let Some(t) = &global.declared_type {
                self.add_type_ref(t, GLOBAL);
            }
            self.walk_expr(&global.initializer, GLOBAL);
        }
    }

    pub(crate) fn walk_struct(&mut self, st: &StructDeclarationNode) {
        for method in &st.methods {
            self.walk_attributes(&method.attributes, GLOBAL);
            self.walk_method(method, &st.name.text);
        }
    }

    pub(crate) fn walk_method(&mut self, func: &FunctionNode, owner: &str) {
        let scope = self.fresh_scope();
        // Instance methods receive an implicit `this` bound to the owning type, so member
        // access on `this` can be resolved to the owner's fields/methods. Static methods do not.
        if !func.is_static {
            self.decls.push(Decl {
                name: "this".to_string(),
                kind: SymKind::Param,
                detail: format!("(this) {}", owner),
                doc_comment: None,
                start: func.name.position.start,
                end: func.name.position.end,
                scope,
                ty: Some(owner.to_string()),
                is_main: self.is_main,
                file_path: self.current_file.clone(),
            });
            let mut owner_token = func.name.clone();
            owner_token.text = owner.to_string();
            self.record_decl_type(&Type::Struct(owner_token, None));
        }
        self.walk_params_and_body(func, scope);
    }

    pub(crate) fn walk_function(&mut self, func: &FunctionNode, _owner: Option<&str>) {
        let scope = self.fresh_scope();
        self.walk_params_and_body(func, scope);
    }

    pub(crate) fn walk_params_and_body(&mut self, func: &FunctionNode, scope: usize) {
        for param in &func.parameters {
            let ty = param.type_.display_name();
            let detail = format!("(parameter) {}: {}", param.name.text, ty);
            self.push_decl(&param.name, SymKind::Param, detail, scope, Some(ty));
            self.record_decl_type(&param.type_);
            self.add_type_ref(&param.type_, scope);
        }
        if let Some(rt) = &func.return_type {
            self.add_type_ref(rt, scope);
        }

        for stmt in func.body {
            self.walk_stmt(stmt, scope);
        }
    }
}
