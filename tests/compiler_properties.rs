#![cfg(feature = "native")]

use crate::common;

use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_hir::TargetLayout;
use dream_mir::backend::Target;
use dream_mir::passes::{MirDump, PassManager};
use dream_sema::{analyzer::Analyzer, module_graph::ModuleGraph};
use dream_syntax::{lexer::Lexer, parser::Parser};
use proptest::prelude::*;

fn verified_ir(source: &str, target: Target) -> Option<String> {
    let mut diagnostics = DiagnosticBag::new(None);
    let parse_arena = Bump::new();
    let mut parser = Parser::new(
        Lexer::new(source.to_owned()),
        &parse_arena,
        &mut diagnostics,
    );
    let tree = parser.parse().expect("parser must recover to a program");
    if diagnostics.has_errors() {
        return None;
    }
    let graph = ModuleGraph::single(tree.get_root().clone());
    let arena = Bump::new();
    let mut analyzer = Analyzer::new(&graph, &arena).with_target_layout(TargetLayout {
        ptr_size: target.spec().ptr_size,
        ptr_align: target.spec().ptr_align,
    });
    let analyzed = analyzer.analyze(&mut diagnostics);
    if diagnostics.has_errors() {
        return None;
    }
    let hir = analyzed
        .expect("analyzer failure must report a diagnostic")
        .hir;
    let interner = analyzer.interner();
    let mut mir = dream_mir::lower::lower_program(&hir, interner);
    let mut dump = MirDump::disabled();
    dream_mir::passes::optimize_module_opts(&mut mir, interner, true, &mut dump);
    dream_mir::passes::run_function_pipelines(
        &mut mir,
        interner,
        &PassManager::release_pipeline(),
        &PassManager::async_poll_pipeline(),
        &mut dump,
    );
    dream_mir::passes::run_late_module_passes(&mut mir, interner, &mut dump);
    let violations = dream_mir::verify::verify_module(&mir, interner);
    assert!(
        violations.is_empty(),
        "source:\n{}\nviolations: {:?}",
        source,
        violations
    );
    Some(common::emit_ll_for(&mir, interner, target))
}

fn expression() -> impl Strategy<Value = String> {
    (-20i32..=20)
        .prop_map(|value| value.to_string())
        .prop_recursive(3, 24, 2, |inner| {
            (
                inner.clone(),
                prop::sample::select(vec!["+", "-", "*"]),
                inner,
            )
                .prop_map(|(left, op, right)| format!("({left} {op} {right})"))
        })
}

fn valid_program() -> impl Strategy<Value = String> {
    (expression(), 0u8..8, any::<bool>()).prop_map(|(value, iterations, managed)| {
        let declarations = if managed {
            "class Box { public value: int; public constructor(value: int) { this.value = value; } }"
        } else { "" };
        let ownership = if managed { "let box = Box(result); result = box.value;" } else { "" };
        format!("{declarations}\nfun identity<T>(value: T): T {{ return value; }}\nfun main(): int {{ let result = identity<int>({value}); for (let i = 0; i < {iterations}; i++) {{ if (i % 2 == 0) {{ result += i; }} else {{ result -= i; }} }} {ownership} return result; }}")
    })
}

fn property_config() -> ProptestConfig {
    let mut config = ProptestConfig {
        timeout: 120_000,
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::Direct(
                "tests/compiler_properties.proptest-regressions",
            ),
        )),
        ..ProptestConfig::default()
    };
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = 32;
    }
    config
}

proptest! {
    #![proptest_config(property_config())]

    #[test]
    fn valid_sources_verify_and_emit_identical_ir(source in valid_program()) {
        for target in [Target::native(), Target::wasm32()] {
            let first = verified_ir(&source, target.clone());
            prop_assert!(first.is_some(), "generated valid source was rejected: {}", source);
            prop_assert_eq!(first, verified_ir(&source, target));
        }
    }

    #[test]
    fn arbitrary_source_never_panics(source in prop::collection::vec(any::<char>(), 0..256)
        .prop_map(|chars| chars.into_iter().collect::<String>())) {
        // Valid survivors must pass the same verifier/emitter path as generated programs.
        let _ = verified_ir(&source, Target::native());
    }

    #[test]
    fn mutated_programs_never_panic(
        source in valid_program(), offset in any::<usize>(), replacement in prop::sample::select(
            vec!["", "?", "{", "}", "undefined", "\"", "await", "💥"]
        )
    ) {
        let (index, original) = source.char_indices()
            .nth(offset % source.chars().count())
            .expect("generated programs are nonempty");
        let mut mutated = source;
        mutated.replace_range(index..index + original.len_utf8(), replacement);
        let _ = verified_ir(&mutated, Target::native());
    }
}

#[test]
fn managed_property_regression() {
    let source = "class Box { public value: int; public constructor(value: int) { this.value = value; } } fun identity<T>(value: T): T { return value; } fun main(): int { let result = identity<int>(0); for (let i = 0; i < 0; i++) { if (i % 2 == 0) { result += i; } else { result -= i; } } let box = Box(result); result = box.value; return result; }";
    assert!(verified_ir(source, Target::native()).is_some());
}
