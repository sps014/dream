use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_sema::analyzer::Analyzer;
use dream_syntax::{lexer::Lexer, parser::Parser};
use std::rc::Rc;

fn module_function_symbol(other_name: &str) -> String {
    let arena = Bump::new();
    let mut diagnostics = DiagnosticBag::new(None);
    let inputs = [("a", "foo"), ("b", other_name)]
        .iter()
        .map(|(module, name)| {
            let source = format!("module {module}; fun {name}(): int {{ return 1; }}");
            let parsed = Parser::new(Lexer::new(source.clone()), &arena, &mut diagnostics)
                .parse()
                .unwrap();
            let mut program = parsed.get_root().clone();
            let path = format!("{module}.dream");
            program.functions[0].file_path = Some(Rc::from(path.as_str()));
            (path, source, program)
        })
        .collect();
    let graph = dream_sema::module_graph::ModuleGraph::new(inputs, &indexmap::IndexMap::new());
    let mut analyzer = Analyzer::new(&graph, &arena);
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
fn parameter_ownership_modes_are_semantic_hir_facts() {
    let source = format!(
        "{}\n{}",
        common::SYSTEM_STUB,
        r#"
        class Owned { public constructor() {} }
        shared class Shared { public constructor() {} }
        fun modes(borrow borrowed: Owned, sink: Owned, shared_value: Shared, ref number: int): void {}
        fun main(): void {}
    "#
    );
    common::compile_test_pipeline(&source, |hir, _| {
        let function = hir
            .functions
            .iter()
            .find(|function| function.name == "modes")
            .unwrap();
        let modes: Vec<_> = function
            .params
            .iter()
            .map(|parameter| parameter.mode)
            .collect();
        assert_eq!(
            modes,
            vec![
                dream_hir::ParamMode::Borrow,
                dream_hir::ParamMode::Sink,
                dream_hir::ParamMode::Share,
                dream_hir::ParamMode::Ref
            ]
        );
    });
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
        assert!(!ir.contains("@s0_5_Label_0_to_string("));
        assert!(!ir.contains("@s0_5_Label_0_hash_code("));
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
        assert!(ir.contains("@s0_5_Label_0_to_string("));
    });
}
#[test]
fn interface_dispatch_uses_resolved_definitions_after_symbol_rename() {
    let source = format!(
        "{}\n{}",
        common::SYSTEM_STUB,
        r#"
        interface Named { fun name(): string; }
        class Label : Named {
            public constructor() {}
            public fun name(): string { return "label"; }
        }
        class Other : Named {
            public constructor() {}
            public fun name(): string { return "other"; }
        }
        fun display(value: Named): string { return value.name(); }
        fun main(): void { System.println(display(Label())); }
    "#
    );
    common::compile_test_pipeline(&source, |hir, interner| {
        let definitions: Vec<_> = hir
            .interfaces
            .impls
            .iter()
            .flat_map(|imp| imp.entries.iter())
            .flat_map(|(_, defs)| defs.iter().flatten().copied())
            .collect();
        assert_eq!(definitions.len(), 2);
        let mut mir = dream_mir::lower::lower_program(hir, interner);
        for (idx, def) in definitions.iter().enumerate() {
            let function = mir.functions.iter_mut().find(|f| f.def == *def).unwrap();
            function.name = format!("unrelated_method_{idx}");
            function.symbol = format!("resolved_interface_method_{idx}");
        }
        dream_mir::passes::optimize_module(&mut mir, interner);
        for def in definitions {
            assert!(mir.functions.iter().any(|f| f.def == def));
        }
        let ir = common::emit_ll(&mir, interner);
        assert!(ir.contains("@resolved_interface_method_0("));
        assert!(ir.contains("@resolved_interface_method_1("));
    });
}
mod common;
