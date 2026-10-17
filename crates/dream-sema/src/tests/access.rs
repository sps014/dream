use super::harness::assert_no_type_errors;
use crate::{analyzer::Analyzer, module_graph::ModuleGraph};
use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::{lexer::Lexer, parser::Parser};
use indexmap::IndexMap;

fn analyze_sources(generated: bool, declaration: &str, caller: &str) -> DiagnosticBag {
    let arena = Bump::new();
    let mut diagnostics = DiagnosticBag::new(None);
    let sources = [("library.dream", declaration), ("caller.dream", caller)];
    let mut inputs = Vec::new();
    for (file, source) in sources {
        let tree = Parser::new(Lexer::new(source.to_string()), &arena, &mut diagnostics)
            .parse()
            .unwrap();
        let mut program = tree.get_root().clone();
        for function in &mut program.functions {
            function.file_path = Some(file.into());
        }
        for ty in &mut program.structs {
            ty.file_path = Some(file.into());
            for method in &mut ty.methods {
                method.file_path = Some(file.into());
            }
        }
        inputs.push((file.to_string(), source.to_string(), program));
    }
    let mut edges = IndexMap::new();
    edges.insert(
        "caller.dream".to_string(),
        vec!["library.dream".to_string()],
    );
    let mut graph = ModuleGraph::new(inputs, &edges);
    if generated {
        graph.generated_files.insert("caller.dream".to_string());
    }
    let hir_arena = Bump::new();
    let mut analyzer = Analyzer::new(&graph, &hir_arena);
    let _ = analyzer.analyze(&mut diagnostics);
    diagnostics
}

fn analyze_member_call(generated: bool, visibility: &str) -> DiagnosticBag {
    analyze_sources(
        generated,
        &format!(
            "module library; public class Service {{ {visibility} static fun answer(): int {{ return 42; }} }}"
        ),
        "module caller; fun main(): void { let result = Service.answer(); }",
    )
}

#[test]
fn generic_parameters_shadow_inaccessible_class_names_in_signatures() {
    assert_no_type_errors(&analyze_sources(
        false,
        "module library; public class Service { public static fun identity<T>(value: T): T { return value; } }",
        "class T {} fun main(): void { let value = Service.identity<int>(42); }",
    ));
}

#[test]
fn internal_class_annotations_are_visible_only_to_generated_callers() {
    let declaration = "module library; internal class Hidden {}";
    let caller = "module caller; fun use(value: Hidden): void {} fun main(): void {}";
    assert_no_type_errors(&analyze_sources(true, declaration, caller));
    let diagnostics = analyze_sources(false, declaration, caller);
    assert!(
        diagnostics
            .errors()
            .any(|d| d.message.contains("not 'public'"))
    );
}

#[test]
fn generated_code_can_call_internal_members() {
    assert_no_type_errors(&analyze_member_call(true, "internal"));
}

#[test]
fn handwritten_code_cannot_call_internal_members() {
    let diagnostics = analyze_member_call(false, "internal");
    assert!(diagnostics.has_errors());
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .any(|d| d.message.contains("answer"))
    );
}

#[test]
fn generated_code_cannot_call_private_members() {
    let diagnostics = analyze_member_call(true, "");
    assert!(diagnostics.has_errors());
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .any(|d| d.message.contains("private"))
    );
}
