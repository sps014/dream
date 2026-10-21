//! Owned pre-generation facts. Source identities stay independent of HIR and the driver.
use dream_diagnostics::Diagnostic;
use dream_syntax::nodes::Type;
use indexmap::IndexMap;

#[derive(Debug, Clone)]
pub struct GenFact {
    pub ty: Type,
    pub constant: Option<GenConstant>,
    pub symbol: String,
    pub symbol_kind: String,
    pub overload: String,
    pub type_args: Vec<Type>,
}

impl Default for GenFact {
    fn default() -> Self {
        Self {
            ty: Type::Unknown,
            constant: None,
            symbol: String::new(),
            symbol_kind: String::new(),
            overload: String::new(),
            type_args: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct GenFacts {
    pub expressions: IndexMap<(String, usize), GenFact>,
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: Vec<GenTypeView>,
    pub declarations: IndexMap<(String, usize), GenDeclarationFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenConstant {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct GenTypeView {
    pub id: String,
    pub ty: Type,
    pub bases: Vec<Type>,
    pub fields: Vec<(String, Type)>,
    pub methods: Vec<GenMethodView>,
    pub variants: Vec<(String, Vec<(String, Type)>)>,
}

#[derive(Debug, Clone)]
pub struct GenMethodView {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub ret: Type,
    pub is_static: bool,
    pub is_async: bool,
}

#[derive(Debug, Clone)]
pub struct GenDeclarationFact {
    pub params: Vec<(String, Type)>,
    pub ret: Type,
    pub overload: String,
    pub overloaded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bumpalo::Bump;
    use dream_diagnostics::DiagnosticBag;
    use dream_syntax::{lexer::Lexer, parser::Parser};

    #[test]
    fn check_only_switch_labels_keep_constant_validation() {
        for (labels, expected) in [
            ("case -1: return; case 0, 1: return;", None),
            (
                "case 1: return; case 1: return;",
                Some("duplicate case label '1'"),
            ),
            (
                "case input: return;",
                Some("switch case labels must be constant"),
            ),
        ] {
            let arena = Bump::new();
            let source =
                format!("fun choose(input: int): void {{ switch (input) {{ {labels} }} }}");
            let mut diagnostics = DiagnosticBag::new(None);
            let mut parser = Parser::new(Lexer::new(source.clone()), &arena, &mut diagnostics);
            let program = parser.parse().unwrap().get_root().clone();
            assert!(!diagnostics.has_errors());
            let graph = crate::module_graph::ModuleGraph::new(
                vec![("switch.dream".into(), source, program)],
                &IndexMap::new(),
            );
            let facts = crate::analyzer::Analyzer::new(&graph, &arena).analyze_for_generators();
            let messages: Vec<_> = facts
                .diagnostics
                .iter()
                .map(|d| d.message.as_str())
                .collect();
            if let Some(expected) = expected {
                assert!(
                    messages.iter().any(|m| m.contains(expected)),
                    "{messages:?}"
                );
            } else {
                assert!(messages.is_empty(), "{messages:?}");
            }
        }
    }

    #[test]
    fn facts_survive_poison_and_preserve_overload_and_constants() {
        let arena = Bump::new();
        let source = "interface View<T> { fun read(): T; } class Box<T> : View<T> { public value: T; public constructor(value: T) { this.value = value; } public fun read(): T { return this.value; } } fun pick(x: int): int { return x; }\nfun pick(x: string): string { return x; }\nfun main(): void { let inferred = 1 + 2; let selected = pick(inferred); let box = Box<int>(5); let read = box.read(); let poison = not_generated_yet(); }";
        let mut diagnostics = DiagnosticBag::new(Some("facts.dream".into()));
        let mut parser = Parser::new(Lexer::new(source.into()), &arena, &mut diagnostics);
        let mut program = parser.parse().unwrap().get_root().clone();
        for f in &mut program.functions {
            f.file_path = Some(std::rc::Rc::from("facts.dream"));
        }
        assert!(!diagnostics.has_errors());
        let graph = crate::module_graph::ModuleGraph::new(
            vec![("facts.dream".into(), source.into(), program)],
            &IndexMap::new(),
        );
        let facts = crate::analyzer::Analyzer::new(&graph, &arena).analyze_for_generators();
        assert!(facts.symbols.iter().any(|view| {
            view.id == "::Box"
                && view
                    .fields
                    .iter()
                    .any(|(name, ty)| name == "value" && ty.get_type() == "int")
                && view
                    .methods
                    .iter()
                    .any(|method| method.name == "read" && method.ret.get_type() == "int")
                && view
                    .bases
                    .iter()
                    .any(|base| base.display_name() == "View<int>")
        }));
        assert!(!diagnostics.has_errors(), "scratch diagnostics escaped");
        assert!(
            facts
                .diagnostics
                .iter()
                .any(|d| d.message.contains("not_generated_yet"))
        );
        assert!(facts.expressions.values().any(
            |f| f.ty.get_type() == "int" && f.constant.as_ref().is_some_and(|c| c.value == "3")
        ));
        assert!(
            facts
                .expressions
                .values()
                .any(|f| f.symbol == "::pick" && !f.overload.is_empty())
        );
        assert!(
            facts
                .expressions
                .values()
                .any(|f| f.ty.is_unknown() && f.symbol.is_empty())
        );
    }
}
