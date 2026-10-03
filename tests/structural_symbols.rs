use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_sema::analyzer::Analyzer;
use dream_syntax::{lexer::Lexer, parser::Parser, syntax_tree::SyntaxTree};
use std::rc::Rc;

fn module_function_symbol(other_name: &str) -> String {
    let arena = Bump::new();
    let mut diagnostics = DiagnosticBag::new(None);
    let source = format!("fun foo(): int {{ return 1; }} fun {other_name}(): int {{ return 2; }}");
    let parsed = Parser::new(Lexer::new(source), &arena, &mut diagnostics)
        .parse()
        .unwrap();
    let mut program = parsed.get_root().clone();
    program.functions[0].file_path = Some(Rc::from("a.dream"));
    program.functions[1].file_path = Some(Rc::from("b.dream"));
    let tree = SyntaxTree::new(program);
    let modules = [
        (Rc::from("a.dream"), Rc::from("a")),
        (Rc::from("b.dream"), Rc::from("b")),
    ]
    .iter()
    .cloned()
    .collect();
    let mut analyzer = Analyzer::new(&tree, &arena).with_file_modules(modules);
    let info = analyzer.analyze(&mut diagnostics).unwrap();
    assert!(!diagnostics.has_errors());
    info.hir
        .functions
        .iter()
        .find(|f| f.file.as_deref() == Some("a.dream"))
        .unwrap()
        .symbol
        .clone()
}

#[test]
fn unrelated_module_name_collision_does_not_rename_existing_symbol() {
    assert_eq!(
        module_function_symbol("bar"),
        dream_types::function_symbol(Some("a"), "foo", &[])
    );
    assert_eq!(module_function_symbol("bar"), module_function_symbol("foo"));
}

#[test]
fn object_protocol_uses_resolved_definitions_after_symbol_rename() {
    let source = format!(
        "{}\n{}",
        common::SYSTEM_STUB,
        r#"
        class Label {
            public constructor() {}
            public override fun to_string(): string { return "label"; }
            public override fun hash_code(): int { return 7; }
        }
        fun main(): void {
            let label = Label();
            System.println(label);
            System.println(label.hash_code());
        }
    "#
    );
    common::compile_test_pipeline(&source, |hir, interner| {
        let ty = *hir
            .layouts
            .structs
            .iter()
            .find(|(_, l)| l.name == "Label")
            .unwrap()
            .0;
        let methods = hir.object_methods[&ty];
        let mut mir = dream_mir::lower::lower_program(hir, interner);
        for (def, symbol) in [
            (methods.to_string.unwrap(), "custom_display"),
            (methods.hash_code.unwrap(), "custom_hash"),
        ] {
            let function = mir.functions.iter_mut().find(|f| f.def == def).unwrap();
            function.name = format!("unrelated_{symbol}");
            function.symbol = symbol.into();
        }
        dream_mir::passes::optimize_module(&mut mir, interner);
        let ir = common::emit_ll(&mir, interner);
        assert!(ir.contains("@custom_display("));
        assert!(ir.contains("@custom_hash("));
        assert!(!ir.contains("@Label_to_string("));
        assert!(!ir.contains("@Label_hash_code("));
    });
}

#[test]
fn protocol_override_survives_generic_only_type_reachability() {
    let source = format!(
        "{}\n{}",
        common::SYSTEM_STUB,
        r#"
        class Label {
            public override fun to_string(): string { return "label"; }
        }
        class Holder<T> { public constructor() {} }
        fun touch(value: Holder<Label>): int { return 1; }
        fun main(): void { System.println(touch(Holder<Label>())); }
    "#
    );
    common::compile_test_pipeline(&source, |hir, interner| {
        let ty = *hir
            .layouts
            .structs
            .iter()
            .find(|(_, l)| l.name == "Label")
            .unwrap()
            .0;
        let formatter = hir.object_methods[&ty].to_string.unwrap();
        let mut mir = dream_mir::lower::lower_program(hir, interner);
        dream_mir::passes::optimize_module(&mut mir, interner);
        assert!(mir.layouts.structs.contains_key(&ty));
        assert!(mir.functions.iter().any(|f| f.def == formatter));
        let ir = common::emit_ll(&mir, interner);
        assert!(ir.contains("@Label_to_string("));
    });
}
mod common;
