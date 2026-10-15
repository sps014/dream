use super::{Analyzer, ide::IdeTarget};
use crate::gen_facts::{GenFact, GenFacts};
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{ExpressionNode, Type};
use dream_text::text_span::TextSpan;

impl<'a> Analyzer<'a> {
    /// Analyzes the user's compilation into owned facts; diagnostics remain in the returned table.
    pub fn analyze_for_generators(mut self) -> GenFacts {
        self.generator_facts = Some(GenFacts::default());
        let mut scratch = DiagnosticBag::new(None);
        let _ = self.analyze_frontend(self.program, &mut scratch);
        let mut facts = self.generator_facts.take().unwrap_or_default();
        for reference in &self.ide_refs {
            let Some(file) = &reference.file else {
                continue;
            };
            let fact = facts
                .expressions
                .entry((file.clone(), reference.start))
                .or_default();
            let (symbol, kind) = self.generator_symbol(&reference.target);
            if let Some(ty) = reference.result.ty() {
                fact.ty = self.type_ctx.syntax_type(ty);
            }
            fact.symbol = symbol;
            fact.symbol_kind = kind.to_string();
            if let Some(identity) = callee(&reference.target) {
                fact.overload = format!(
                    "{}#{}",
                    fact.symbol,
                    self.function_table.emitted_name(&self.type_ctx, identity)
                );
                fact.type_args = identity
                    .1
                    .iter()
                    .map(|t| self.type_ctx.syntax_type(*t))
                    .collect();
            }
        }
        facts
            .expressions
            .sort_by(|(a_file, a_start), _, (b_file, b_start), _| {
                a_file.cmp(b_file).then(a_start.cmp(b_start))
            });
        for file in &self.graph.files {
            for function in &file.program.functions {
                let Some(identity) = self.function_table.declaration_node(function) else {
                    continue;
                };
                let module = &self.graph.modules[file.module.0 as usize].path;
                let symbol = dream_abi::attributes::decl_identity(module, &function.name.text);
                let overloaded = self
                    .graph
                    .files
                    .iter()
                    .filter(|other| other.module == file.module)
                    .flat_map(|other| &other.program.functions)
                    .filter(|other| other.name.text == function.name.text)
                    .count()
                    > 1;
                facts.declarations.insert(
                    (file.path.clone(), function.name.position.start),
                    crate::gen_facts::GenDeclarationFact {
                        params: self.generator_signature_params(&identity),
                        ret: self
                            .function_table
                            .functions
                            .get(&identity)
                            .map(|f| self.type_ctx.syntax_type(f.resolved_return))
                            .unwrap_or(Type::Unknown),
                        overload: format!(
                            "{symbol}#{}",
                            self.function_table.emitted_name(&self.type_ctx, &identity)
                        ),
                        overloaded,
                    },
                );
            }
        }
        for ((_, _), candidates) in &self.function_table.methods {
            for identity in candidates {
                let Some(source) = self.ide_sources.get(&identity.0) else {
                    continue;
                };
                let Some(file) = &source.file else { continue };
                let target = IdeTarget::Callee {
                    key: identity.clone(),
                    label: String::new(),
                };
                let target = IdeTarget::Resolved {
                    def: identity.0,
                    source: source.clone(),
                    target: Box::new(target),
                };
                let symbol = self.generator_symbol(&target).0;
                facts
                    .declarations
                    .entry((file.clone(), source.start))
                    .or_insert_with(|| crate::gen_facts::GenDeclarationFact {
                        params: self.generator_signature_params(identity),
                        ret: self
                            .function_table
                            .functions
                            .get(identity)
                            .map(|f| self.type_ctx.syntax_type(f.resolved_return))
                            .unwrap_or(Type::Unknown),
                        overload: format!(
                            "{symbol}#{}",
                            self.function_table.emitted_name(&self.type_ctx, identity)
                        ),
                        overloaded: candidates.len() > 1,
                    });
            }
        }
        facts.symbols = self.generator_symbol_views();
        facts.diagnostics = scratch.diagnostics;
        facts
    }

    fn generator_signature_params(
        &self,
        identity: &crate::function_table::FunctionIdentity,
    ) -> Vec<(String, Type)> {
        self.function_table
            .functions
            .get(identity)
            .map(|f| {
                f.param_names
                    .iter()
                    .zip(&f.parameters)
                    .map(|(name, ty)| (name.clone(), self.type_ctx.syntax_type(*ty)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Associates an expression result and exact constant with its original source anchor.
    pub(super) fn record_generator_type(&mut self, expression: &ExpressionNode<'_>, ty: Type) {
        let Some(span) = expression.position() else {
            return;
        };
        let constant = (!ty.is_unknown())
            .then(|| self.generator_constant(expression, &ty))
            .flatten();
        self.record_generator_type_at(span, ty);
        if let Some(facts) = &mut self.generator_facts {
            let key = (
                self.current_file
                    .as_ref()
                    .map(|f| f.to_string())
                    .unwrap_or_default(),
                span.start,
            );
            if let Some(fact) = facts.expressions.get_mut(&key) {
                fact.constant = constant;
            }
        }
    }

    /// Records a type independently of HIR so poisoned analyses remain queryable.
    pub(super) fn record_generator_type_at(&mut self, span: TextSpan, ty: Type) {
        if let Some(facts) = &mut self.generator_facts {
            let file = self
                .current_file
                .as_ref()
                .map(|p| p.to_string())
                .unwrap_or_default();
            facts
                .expressions
                .entry((file, span.start))
                .or_insert_with(GenFact::default)
                .ty = ty;
        }
    }

    fn generator_symbol(&self, target: &IdeTarget) -> (String, &'static str) {
        match target {
            IdeTarget::Resolved { source, target, .. } => {
                let kind = self.generator_symbol(target).1;
                for file in &self.graph.files {
                    if source.file.as_deref() != Some(&file.path) {
                        continue;
                    }
                    let module = &self.graph.modules[file.module.0 as usize].path;
                    let id = |name: &str| dream_abi::attributes::decl_identity(module, name);
                    for f in &file.program.functions {
                        if f.name.position.start == source.start {
                            return (id(&f.name.text), kind);
                        }
                    }
                    for d in &file.program.structs {
                        if d.name.position.start == source.start {
                            return (id(&d.name.text), kind);
                        }
                        for m in &d.methods {
                            if m.name.position.start == source.start {
                                return (format!("{}.{}", id(&d.name.text), m.name.text), "method");
                            }
                        }
                    }
                    for d in &file.program.enums {
                        if d.name.position.start == source.start {
                            return (id(&d.name.text), kind);
                        }
                        for m in &d.methods {
                            if m.name.position.start == source.start {
                                return (format!("{}.{}", id(&d.name.text), m.name.text), "method");
                            }
                        }
                    }
                }
                self.generator_symbol(target)
            }
            IdeTarget::Local { name } => (name.clone(), "local"),
            IdeTarget::Global { name } => (name.clone(), "global"),
            IdeTarget::Callee { key, .. } => {
                let def = self.type_ctx.defs.get(key.0);
                (
                    dream_abi::attributes::decl_identity(&def.module_path, &def.name),
                    "function",
                )
            }
            IdeTarget::Constructor { ty, .. } => (self.generator_type_symbol(*ty), "type"),
            IdeTarget::Field { owner, name } => (
                format!("{}.{}", self.generator_type_symbol(*owner), name),
                "field",
            ),
            IdeTarget::EnumMember { owner, member } => (
                format!("{}.{}", self.generator_type_symbol(*owner), member),
                "variant",
            ),
            IdeTarget::UnionVariant { owner, variant } => (
                format!("{}.{}", self.generator_type_symbol(*owner), variant),
                "variant",
            ),
            IdeTarget::Expr => (String::new(), ""),
        }
    }

    fn generator_type_symbol(&self, ty: dream_types::TypeId) -> String {
        use dream_types::TyKind;
        let def = match self.type_ctx.interner.kind(ty) {
            TyKind::Struct(d, _)
            | TyKind::Union(d, _)
            | TyKind::Interface(d, _)
            | TyKind::Enum(d) => self.type_ctx.defs.get(*d),
            _ => return String::new(),
        };
        dream_abi::attributes::decl_identity(&def.module_path, &def.name)
    }
}

fn callee(target: &IdeTarget) -> Option<&crate::function_table::FunctionIdentity> {
    match target {
        IdeTarget::Resolved { target, .. } => callee(target),
        IdeTarget::Callee { key, .. } => Some(key),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bumpalo::Bump;
    use dream_syntax::{lexer::Lexer, parser::Parser};

    #[test]
    fn facts_only_analysis_does_not_collect_functions_or_globals() {
        let arena = Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);
        let mut parser = Parser::new(
            Lexer::new("let global: int = 42; fun main(): void { let value = global; }".into()),
            &arena,
            &mut diagnostics,
        );
        let program = parser.parse().unwrap().get_root().clone();
        let graph = crate::module_graph::ModuleGraph::single(program);
        let mut analyzer = Analyzer::new(&graph, &arena);
        analyzer.generator_facts = Some(GenFacts::default());
        analyzer
            .analyze_frontend(analyzer.program, &mut diagnostics)
            .unwrap();
        assert!(analyzer.hir.functions.is_empty());
        assert!(analyzer.hir.global_decls.is_empty());
    }
}
