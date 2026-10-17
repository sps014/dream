use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::{lexer::Lexer, parser::Parser};
use heck::ToSnakeCase;

#[test]
fn embedded_sources_declare_one_type_named_after_the_file() {
    for package in crate::STD_PACKAGES {
        for &(path, source) in package.files {
            let arena = Bump::new();
            let mut diagnostics = DiagnosticBag::new(Some(path.to_string()));
            let tree = Parser::new(Lexer::new(source.to_string()), &arena, &mut diagnostics)
                .parse()
                .expect("parser always returns a tree");
            assert!(
                !diagnostics.has_errors(),
                "{path}: {:?}",
                diagnostics.diagnostics
            );
            let root = tree.get_root();
            let names: Vec<_> = root
                .structs
                .iter()
                .map(|t| &t.name.text)
                .chain(root.enums.iter().map(|t| &t.name.text))
                .chain(root.interfaces.iter().map(|t| &t.name.text))
                .collect();
            assert!(names.len() <= 1, "{path} declares {names:?}");
            if let Some(name) = names.first() {
                for extension in &root.extends {
                    assert_eq!(&extension.target.text, *name, "{path} extends another type");
                }
                assert_eq!(
                    std::path::Path::new(path)
                        .file_stem()
                        .unwrap()
                        .to_str()
                        .unwrap(),
                    name.to_snake_case(),
                    "{path}: {name}"
                );
            }
        }
    }
    for package in crate::BOOTSTRAP_PACKAGES {
        assert!(crate::STD_PACKAGES.iter().any(|p| p.name == *package));
    }
}

#[test]
fn type_file_names_preserve_acronym_boundaries() {
    assert_eq!("JsonKind".to_snake_case(), "json_kind");
    assert_eq!("HTTPServer".to_snake_case(), "http_server");
    assert_eq!("Utf8Decoder".to_snake_case(), "utf8_decoder");
}
