use crate::driver::source_loader::ProgramAccumulator;
use dream_diagnostics::{Diagnostic, DiagnosticBag};
use dream_syntax::nodes::ProgramNode;
use dream_syntax::token::syntax_token::SyntaxToken;
use indexmap::IndexMap;

pub(super) fn check(
    acc: &ProgramAccumulator<'_>,
    program: &ProgramNode<'_>,
    path: &str,
    generator: &str,
    diagnostics: &mut DiagnosticBag,
) {
    let module = program
        .module
        .as_ref()
        .map(|m| m.path.text.as_str())
        .unwrap_or_default();
    let mut names = IndexMap::new();
    let mut add = |name: &SyntaxToken, file: Option<&str>| {
        if super::decls::module_of(acc, file) == module {
            names
                .entry(name.text.clone())
                .or_insert((file.map(str::to_string), name.position));
        }
    };
    for d in &acc.all_structs {
        add(&d.name, d.file_path.as_deref());
    }
    for d in &acc.all_enums {
        add(&d.name, d.file_path.as_deref());
    }
    for d in &acc.all_interfaces {
        add(&d.name, d.file_path.as_deref());
    }
    for d in &acc.all_functions {
        add(&d.name, d.file_path.as_deref());
    }
    for d in &acc.all_globals {
        add(&d.name, d.file_path.as_deref());
    }
    let generated = program
        .structs
        .iter()
        .map(|d| &d.name)
        .chain(program.enums.iter().map(|d| &d.name))
        .chain(program.interfaces.iter().map(|d| &d.name))
        .chain(program.functions.iter().map(|d| &d.name))
        .chain(program.globals.iter().map(|d| &d.name));
    for name in generated {
        if let Some((file, span)) = names.get(&name.text) {
            diagnostics.report(Diagnostic::new(
                format!("generated declaration '{}' collides with an existing declaration in module '{module}'", name.text),
                Some(*span), file.clone(),
            ).with_note(format!("generator '{generator}' emitted the conflicting declaration in {path}")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bumpalo::Bump;
    use dream_syntax::{lexer::Lexer, parser::Parser};

    #[test]
    fn generated_collision_is_anchored_at_the_original_declaration() {
        let arena = Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);
        let mut parser = Parser::new(
            Lexer::new("class Original {}".into()),
            &arena,
            &mut diagnostics,
        );
        let ast = parser.parse().unwrap();
        let mut acc = ProgramAccumulator {
            all_structs: ast.get_root().structs.clone(),
            ..Default::default()
        };
        acc.all_structs[0].file_path = Some(std::rc::Rc::from("user.dream"));
        let mut parser = Parser::new(
            Lexer::new("class Original {}".into()),
            &arena,
            &mut diagnostics,
        );
        let generated = parser.parse().unwrap();
        check(
            &acc,
            generated.get_root(),
            "generated.dream",
            "toy",
            &mut diagnostics,
        );
        assert_eq!(diagnostics.diagnostics.len(), 1);
        let diagnostic = &diagnostics.diagnostics[0];
        assert_eq!(diagnostic.file_path.as_deref(), Some("user.dream"));
        assert!(diagnostic.message.contains("collides"));
        assert!(diagnostic.notes.iter().any(|n| n.message.contains("toy")));
    }
}
