use crate::analyzer::Analyzer;
use crate::module_graph::ModuleGraph;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::{lexer::Lexer, parser::Parser};

#[test]
fn enum_payloads_know_forward_value_struct_identity() {
    for payload in ["Value", "Value<int>"] {
        let source = format!(
            "enum Choice {{ Some({payload}), None }}
             class Owner {{}}
             struct Value{} {{ public owner: Owner; }}
             fun main() {{}}",
            if payload.contains('<') { "<T>" } else { "" }
        );
        let arena = bumpalo::Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);
        let tree = Parser::new(Lexer::new(source), &arena, &mut diagnostics)
            .parse()
            .unwrap();
        let graph = ModuleGraph::single(tree.get_root().clone());
        let mut analyzer = Analyzer::new(&graph, &arena);
        let info = analyzer.analyze(&mut diagnostics).unwrap();
        let ty = *info.hir.layouts.unions.keys().next().unwrap();
        drop(info);
        assert!(!diagnostics.has_errors(), "{:?}", diagnostics.diagnostics);
        assert!(analyzer.interner().is_value_type(ty));
        assert!(!analyzer.interner().is_niche_union(ty));
    }
}

#[test]
fn recursive_inline_enum_payloads_are_diagnostics() {
    for payload in ["Value", "Value<int>"] {
        let source = format!(
            "enum Choice {{ Some({payload}), None }} struct Value{} {{ public choice: Choice; }} fun main() {{}}",
            if payload.contains('<') { "<T>" } else { "" }
        );
        let diagnostics = super::harness::analyze_code(&source);
        assert!(diagnostics.has_errors());
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| d.message.contains("cannot contain itself by value"))
        );
    }
}
