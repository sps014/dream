//! Lightweight parse-only discovery of `@test` functions in a Dream source file.

use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::lexer::Lexer;
use dream_syntax::parser::Parser;

#[derive(Debug, Clone)]
pub struct DiscoveredTest {
    pub name: String,
}

/// Parse `source` and return every top-level `@test` function name (declaration order).
/// Does not type-check — shape is validated later during the synthesized compile.
pub fn discover_tests_in_source(
    path: &str,
    source: &str,
) -> Result<Vec<DiscoveredTest>, String> {
    let arena = Bump::new();
    let mut diagnostics = DiagnosticBag::new(Some(path.to_string()));
    let lexer = Lexer::new(source.to_string());
    let mut parser = Parser::new(lexer, &arena, &mut diagnostics);
    let program = parser
        .parse()
        .map_err(|_| format!("failed to parse '{}'", path))?;
    if diagnostics.has_errors() {
        let mut msg = format!("parse errors in '{}'", path);
        for e in diagnostics.errors() {
            msg.push_str("\n  ");
            msg.push_str(&e.message);
        }
        return Err(msg);
    }
    let root = program.get_root();
    let mut out = Vec::new();
    for f in &root.functions {
        if !dream_abi::attributes::has_test_attr(&f.attributes) {
            continue;
        }
        out.push(DiscoveredTest {
            name: f.name.text.clone(),
        });
    }
    Ok(out)
}
