//! Deep and long programs compile on a small thread stack: every recursive tree walk grows the
//! stack itself instead of relying on the thread it happens to run on.

use crate::common;
use dream_mir::lower::lower_program;

const SMALL_STACK: usize = 1024 * 1024;

fn on_small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(f)
        .expect("spawn compiler thread")
        .join()
        .expect("compiler thread panicked");
}

fn program(expr: &str) -> String {
    format!(
        "class Link {{\n    public fun step(): Link {{\n        return this;\n    }}\n}}\n\nfun main(): int {{\n    let link = Link();\n    let x = {expr};\n    return 0;\n}}\n"
    )
}

fn lowers(expr: String) {
    on_small_stack(move || {
        common::compile_test_pipeline(&program(&expr), |hir, interner| {
            lower_program(hir, interner);
        });
    });
}

#[test]
fn long_binary_chain_compiles() {
    lowers(vec!["1"; 3000].join(" + "));
}

#[test]
fn long_method_chain_compiles() {
    lowers(format!("link{}", ".step()".repeat(2000)));
}

#[test]
fn nesting_just_under_the_limit_compiles() {
    lowers(format!("{}1{}", "(".repeat(254), ")".repeat(254)));
}

#[test]
fn nesting_past_the_limit_is_a_diagnostic() {
    on_small_stack(|| {
        let expr = format!("{}1{}", "(".repeat(300), ")".repeat(300));
        let diagnostics = common::analyze_code(&program(&expr));
        assert!(
            diagnostics
                .errors()
                .any(|d| d.message.contains("Nesting is too deep")),
            "expected a nesting diagnostic, got {:?}",
            diagnostics.errors().collect::<Vec<_>>()
        );
    });
}
