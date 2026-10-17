//! Consumed functions stay in the program check-only: their bodies are type-checked like any
//! other code but emit no HIR, and ordinary code may not call them. `@generator_only`
//! declarations are typed normally but usable only inside consumed functions that carry one of
//! the granting attributes. Uses are collected as names resolve and judged once every
//! declaration, including lazily registered generic instances, is known.

use super::Analyzer;
use super::ide::IdeTarget;
use dream_diagnostics::{Diagnostic, DiagnosticBag};
use dream_syntax::nodes::{AttributeNode, FunctionNode};
use dream_text::text_span::TextSpan;
use dream_types::{DefId, DefKind, ModuleId, TyKind, TypeId};
use indexmap::{IndexMap, IndexSet};

/// What the function being analyzed may use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct GeneratorContext {
    /// Identities of the attributes the function carries.
    carries: Vec<String>,
    /// A generator consumed the function, so it is check-only.
    consumed: bool,
}

struct Grant {
    id: String,
    label: String,
}

struct Declaration {
    paths: Vec<String>,
    scope: ModuleId,
    file: Option<String>,
    span: Option<TextSpan>,
    grants: Vec<Grant>,
}

struct Use {
    file: Option<String>,
    span: TextSpan,
    label: String,
    defs: Vec<DefId>,
    types: Vec<TypeId>,
    context: Option<GeneratorContext>,
}

#[derive(Default)]
pub(super) struct GeneratorScope {
    /// Consumed function definitions and their `module::name` identity.
    consumed: IndexMap<DefId, String>,
    /// Lambdas written inside a function share its context.
    inherited: IndexMap<DefId, GeneratorContext>,
    declarations: IndexMap<DefId, Declaration>,
    current: Option<GeneratorContext>,
    uses: Vec<Use>,
    /// Carrying a granting attribute is enough; nothing is known about consumption yet.
    unverified_consumption: bool,
    /// Methods of generic instances registered after this point resolve their grants at once.
    grants_resolved: bool,
}

impl GeneratorScope {
    fn active(&self) -> bool {
        !self.consumed.is_empty() || !self.declarations.is_empty()
    }

    /// Whether the function whose body is being analyzed was consumed.
    pub(super) fn analyzing_consumed(&self) -> bool {
        self.current.as_ref().is_some_and(|c| c.consumed)
    }
}

fn carried(attributes: &[AttributeNode]) -> Vec<String> {
    attributes
        .iter()
        .filter_map(|a| a.resolved_id.get().cloned())
        .collect()
}

impl Analyzer<'_> {
    /// For editors, which may analyze a file before any build recorded what generators consumed:
    /// a function carrying a granting attribute may use generator-only names.
    pub fn with_unverified_consumption(mut self) -> Self {
        self.generator_scope.unverified_consumption = true;
        self
    }

    /// Remembers a `@generator_only` declaration; its granting attributes resolve once every type
    /// is registered.
    pub(super) fn note_generator_only(
        &mut self,
        def: DefId,
        attributes: &[AttributeNode],
        file: Option<&str>,
    ) {
        let Some(paths) = dream_abi::attributes::generator_only_paths(attributes) else {
            return;
        };
        let span = attributes
            .iter()
            .find(|a| {
                dream_abi::attributes::is_compiler_attribute(
                    a,
                    dream_abi::attributes::GENERATOR_ONLY_ATTR,
                )
            })
            .map(|a| a.name.position);
        let mut declaration = Declaration {
            paths,
            scope: self.graph.module_for_file(file),
            file: file.map(str::to_string),
            span,
            grants: Vec::new(),
        };
        if self.generator_scope.grants_resolved {
            // The template's declaration already reported unknown attribute paths.
            self.resolve_grants(&mut declaration, &mut DiagnosticBag::new(None));
        }
        self.generator_scope.declarations.insert(def, declaration);
    }

    /// Records a top-level function's generator role: consumed (check-only) or generator-only.
    pub(super) fn note_generator_function(&mut self, function: &FunctionNode<'_>, def: DefId) {
        self.note_generator_only(def, &function.attributes, function.file_path.as_deref());
        let module = self.graph.module_for_file(function.file_path.as_deref());
        let id = dream_abi::attributes::decl_identity(
            &self.graph.modules[module.0 as usize].path,
            &function.name.text,
        );
        if self.graph.consumed.contains_key(&id) {
            self.generator_scope.consumed.insert(def, id);
        }
    }

    /// Resolves each `@generator_only` attribute path to the attribute type it names.
    pub(super) fn resolve_generator_grants(&mut self, diagnostics: &mut DiagnosticBag) {
        let mut declarations = std::mem::take(&mut self.generator_scope.declarations);
        for declaration in declarations.values_mut() {
            self.resolve_grants(declaration, diagnostics);
        }
        self.generator_scope.declarations = declarations;
        self.generator_scope.grants_resolved = true;
    }

    fn resolve_grants(&mut self, declaration: &mut Declaration, diagnostics: &mut DiagnosticBag) {
        for path in &declaration.paths {
            let name = match path.rsplit_once('.') {
                Some((module, name)) => format!("{module}::{name}"),
                None => path.clone(),
            };
            let def = if name.contains("::") {
                self.type_ctx.resolve(DefKind::Struct, &name)
            } else {
                self.type_ctx
                    .resolve_from(declaration.scope, DefKind::Struct, &name)
            };
            match def {
                Some(def) => {
                    let info = self.type_ctx.defs.get(def);
                    declaration.grants.push(Grant {
                        id: dream_abi::attributes::decl_identity(&info.module_path, &info.name),
                        label: format!(
                            "@{}",
                            dream_abi::attributes::attribute_use_name(&info.name)
                        ),
                    });
                }
                None => diagnostics.report(Diagnostic::new(
                    format!("@generator_only names unknown attribute type '{path}'"),
                    declaration.span,
                    declaration.file.clone(),
                )),
            }
        }
    }

    fn generator_context_of(&mut self, function: &FunctionNode<'_>) -> GeneratorContext {
        let def = self.function_declaration(function).map(|key| key.0);
        if let Some(context) = def.and_then(|d| self.generator_scope.inherited.get(&d)) {
            return context.clone();
        }
        GeneratorContext {
            carries: carried(&function.attributes),
            consumed: def.is_some_and(|d| self.generator_scope.consumed.contains_key(&d)),
        }
    }

    /// Makes `function` the context its body's uses are judged in; returns the previous one.
    pub(super) fn enter_generator_context(
        &mut self,
        function: &FunctionNode<'_>,
    ) -> Option<GeneratorContext> {
        if !self.generator_scope.active() {
            return self.generator_scope.current.take();
        }
        let context = self.generator_context_of(function);
        self.generator_scope.current.replace(context)
    }

    pub(super) fn leave_generator_context(&mut self, previous: Option<GeneratorContext>) {
        self.generator_scope.current = previous;
    }

    /// A lambda synthesized inside the current function is analyzed later as its own function.
    pub(super) fn inherit_generator_context(&mut self, lambda: DefId) {
        if let Some(context) = &self.generator_scope.current {
            self.generator_scope
                .inherited
                .insert(lambda, context.clone());
        }
    }

    pub(super) fn record_generator_use(&mut self, span: TextSpan, target: &IdeTarget) {
        if !self.generator_scope.active() {
            return;
        }
        let mut defs = Vec::new();
        let mut types = Vec::new();
        let mut label = String::new();
        let mut pending = vec![target];
        while let Some(target) = pending.pop() {
            match target {
                IdeTarget::Resolved { def, target, .. } => {
                    defs.push(*def);
                    pending.push(target);
                }
                IdeTarget::Callee { key, label: name } => {
                    defs.push(key.0);
                    label = name.clone();
                }
                IdeTarget::Constructor { ty, display } => {
                    types.push(*ty);
                    label = display.clone();
                }
                IdeTarget::Field { owner, name } => {
                    types.push(*owner);
                    label = name.clone();
                }
                IdeTarget::EnumMember {
                    owner,
                    member: name,
                }
                | IdeTarget::UnionVariant {
                    owner,
                    variant: name,
                } => {
                    types.push(*owner);
                    label = name.clone();
                }
                IdeTarget::Local { .. } | IdeTarget::Global { .. } | IdeTarget::Expr => {}
            }
        }
        if defs.is_empty() && types.is_empty() {
            return;
        }
        self.generator_scope.uses.push(Use {
            file: self.current_file.as_ref().map(|f| f.to_string()),
            span,
            label,
            defs,
            types,
            context: self.generator_scope.current.clone(),
        });
    }

    /// Indexer desugaring calls the hook through a synthesized span, which records no use.
    pub(super) fn record_indexer_use(
        &mut self,
        receiver: &dream_syntax::nodes::ExpressionNode<'_>,
        hook: &super::declarations::protocol_hooks::ProtocolHook,
        receiver_type: &str,
    ) {
        if let Some(span) = receiver.position() {
            self.record_generator_use(
                span,
                &IdeTarget::Callee {
                    key: hook.identity.clone(),
                    label: format!("{receiver_type}[]"),
                },
            );
        }
    }

    /// Method definitions of generator-only types answer to their owner's declaration.
    fn generator_method_owners(&self) -> IndexMap<DefId, DefId> {
        let mut owners = IndexMap::new();
        for ((owner, _), candidates) in &self.function_table.methods {
            let owner = match self.type_ctx.interner.kind(*owner) {
                TyKind::Struct(def, _) | TyKind::Union(def, _) | TyKind::Interface(def, _) => *def,
                _ => continue,
            };
            if self.generator_scope.declarations.contains_key(&owner) {
                for identity in candidates {
                    owners.insert(identity.0, owner);
                }
            }
        }
        for ((owner, _), method) in &self.function_table.generic_methods {
            if let TyKind::Struct(def, _) | TyKind::Union(def, _) | TyKind::Interface(def, _) =
                self.type_ctx.interner.kind(*owner)
                && self.generator_scope.declarations.contains_key(def)
            {
                owners.insert(*method, *def);
            }
        }
        owners
    }

    /// True when `def` is a `@generator_only` function, or a member of a generator-only type.
    pub(super) fn is_generator_only_function(&self, def: DefId) -> bool {
        self.generator_scope.declarations.contains_key(&def)
            || (!self.generator_scope.declarations.is_empty()
                && self.generator_method_owners().contains_key(&def))
    }

    fn generator_only_in(&self, ty: TypeId, found: &mut IndexSet<DefId>) {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            match self.type_ctx.interner.kind(ty) {
                TyKind::Struct(def, args)
                | TyKind::Union(def, args)
                | TyKind::Interface(def, args) => {
                    if self.generator_scope.declarations.contains_key(def) {
                        found.insert(*def);
                    }
                    pending.extend(args.iter().copied());
                }
                TyKind::Enum(def) => {
                    if self.generator_scope.declarations.contains_key(def) {
                        found.insert(*def);
                    }
                }
                TyKind::Array(inner) => pending.push(*inner),
                TyKind::Func(params, ret) => {
                    pending.extend(params.iter().copied());
                    pending.push(*ret);
                }
                TyKind::Tuple(elems) => pending.extend(elems.iter().copied()),
                TyKind::Prim(_) | TyKind::Object | TyKind::Void | TyKind::Error | TyKind::Js => {}
            }
        }
    }

    fn generator_only_violation(
        &self,
        label: &str,
        declaration: DefId,
        context: Option<&GeneratorContext>,
        span: TextSpan,
        file: Option<String>,
    ) -> Option<Diagnostic> {
        let grants = &self.generator_scope.declarations.get(&declaration)?.grants;
        let granted = context.is_some_and(|c| grants.iter().any(|g| c.carries.contains(&g.id)));
        if granted
            && (self.generator_facts.is_some()
                || self.generator_scope.unverified_consumption
                || context.is_some_and(|c| c.consumed))
        {
            return None;
        }
        let labels: Vec<_> = grants.iter().map(|g| g.label.as_str()).collect();
        let diagnostic = Diagnostic::new(
            format!(
                "'{label}' is only available inside functions marked {}",
                labels.join(" or ")
            ),
            Some(span),
            file,
        );
        Some(if granted {
            diagnostic
                .with_help("this function carries the attribute, but no generator consumed it")
        } else {
            diagnostic
        })
    }

    /// Reports each misuse once, after every body (and generic instance) was analyzed.
    pub(super) fn report_generator_uses(&mut self, diagnostics: &mut DiagnosticBag) {
        if !self.generator_scope.active() {
            return;
        }
        let owners = self.generator_method_owners();
        let mut reported = IndexSet::new();
        let uses = std::mem::take(&mut self.generator_scope.uses);
        for usage in &uses {
            if !reported.insert((usage.file.clone(), usage.span.start)) {
                continue;
            }
            let context = usage.context.as_ref();
            let consumed = usage
                .defs
                .iter()
                .find_map(|def| self.generator_scope.consumed.get(def));
            if let Some(id) = consumed
                && !context.is_some_and(|c| c.consumed)
                && let Some(by) = self.graph.consumed.get(id)
            {
                let mut diagnostic = Diagnostic::new(
                    format!(
                        "'{}' was consumed by generator '{}' and is not available as a function",
                        usage.label, by.generator
                    ),
                    Some(usage.span),
                    usage.file.clone(),
                );
                if !by.help.is_empty() {
                    diagnostic = diagnostic.with_help(by.help.clone());
                }
                diagnostics.report(diagnostic);
                continue;
            }
            let mut declarations: IndexSet<DefId> = usage
                .defs
                .iter()
                .filter_map(|def| {
                    self.generator_scope
                        .declarations
                        .contains_key(def)
                        .then_some(*def)
                        .or_else(|| owners.get(def).copied())
                })
                .collect();
            // A standard-library generic body, such as `GpuBuffer<T>.length` instantiated with a
            // generator-only `T` by a shader parameter, names its type arguments only because of
            // that instantiation; the instantiating code is judged where it names the type.
            let std_body = usage
                .file
                .as_deref()
                .is_some_and(dream_stdlib::is_std_source);
            for ty in &usage.types {
                if std_body {
                    if let TyKind::Struct(def, _)
                    | TyKind::Union(def, _)
                    | TyKind::Interface(def, _) = self.type_ctx.interner.kind(*ty)
                        && self.generator_scope.declarations.contains_key(def)
                    {
                        declarations.insert(*def);
                    }
                } else {
                    self.generator_only_in(*ty, &mut declarations);
                }
            }
            if let Some(diagnostic) = declarations.iter().find_map(|declaration| {
                self.generator_only_violation(
                    &usage.label,
                    *declaration,
                    context,
                    usage.span,
                    usage.file.clone(),
                )
            }) {
                diagnostics.report(diagnostic);
            }
        }
        self.report_generator_only_signatures(diagnostics);
    }

    /// A function signature that names a generator-only type is a use of that type.
    fn report_generator_only_signatures(&mut self, diagnostics: &mut DiagnosticBag) {
        if self.generator_scope.declarations.is_empty() {
            return;
        }
        for function in self.program.functions.iter().copied() {
            let Some(identity) = self.function_table.declaration_node(function) else {
                continue;
            };
            if self.generator_scope.declarations.contains_key(&identity.0) {
                continue;
            }
            let Some(info) = self.function_table.functions.get(&identity) else {
                continue;
            };
            let mut found = IndexSet::new();
            for ty in info.parameters.iter().chain([&info.resolved_return]) {
                self.generator_only_in(*ty, &mut found);
            }
            if found.is_empty() {
                continue;
            }
            let context = self.generator_context_of(function);
            let file = function.file_path.as_deref().map(str::to_string);
            if let Some(diagnostic) = found.iter().find_map(|declaration| {
                let label = self.type_ctx.defs.get(*declaration).name.clone();
                self.generator_only_violation(
                    &label,
                    *declaration,
                    Some(&context),
                    function.name.position,
                    file.clone(),
                )
            }) {
                diagnostics.report(diagnostic);
            }
        }
    }
}
