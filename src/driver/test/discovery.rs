//! Lightweight parse-only discovery of `@test` functions in a Dream source file.

use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::lexer::Lexer;
use dream_syntax::parser::Parser;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredTest {
    pub name: String,
    /// `@timeout(ms)`; the analyzer rejects a malformed one when the runner compiles.
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct DiscoveredFile {
    pub tests: Vec<DiscoveredTest>,
    pub declares_main: bool,
}

/// Parse `source` and return every top-level `@test` function (declaration order). Does not
/// type-check — shape is validated later during the runner compile.
pub fn discover_tests_in_source(path: &str, source: &str) -> Result<DiscoveredFile, String> {
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
    let mut out = DiscoveredFile::default();
    for f in &root.functions {
        if f.name.text == "main" {
            out.declares_main = true;
        }
        if !dream_abi::attributes::has_test_attr(&f.attributes) {
            continue;
        }
        out.tests.push(DiscoveredTest {
            name: f.name.text.clone(),
            timeout_ms: dream_abi::attributes::test_timeout_ms(&f.attributes).flatten(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_tests_timeouts_and_main() {
        let found = discover_tests_in_source(
            "t.dream",
            "@test\nfun a(): void {}\n@test\n@timeout(2500)\nfun b(): void {}\nfun helper(): void {}\nfun main(): void {}\n",
        )
        .unwrap();
        assert_eq!(
            found.tests,
            vec![
                DiscoveredTest {
                    name: "a".into(),
                    timeout_ms: None
                },
                DiscoveredTest {
                    name: "b".into(),
                    timeout_ms: Some(2500)
                },
            ]
        );
        assert!(found.declares_main);
    }
}
