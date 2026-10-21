use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::{lexer::Lexer, parser::Parser};
use heck::ToSnakeCase;

#[test]
fn embedded_sources_declare_one_type_named_after_the_file() {
    let mut stems = std::collections::BTreeSet::new();
    let mut violations = Vec::new();
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
            let stem = std::path::Path::new(path)
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            if !stems.insert(stem.clone()) {
                violations.push(format!("{path}: file name `{stem}` is not unique"));
            }
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
                assert_eq!(stem, name.to_snake_case(), "{path}: {name}");
            } else {
                // A type-less file extends at most one type and is named after it, optionally
                // qualified (`io/system_io.dream`, `host_system.dream`). Files with neither types
                // nor extensions hold free functions only.
                let mut targets: Vec<_> = root.extends.iter().map(|e| &e.target.text).collect();
                targets.dedup();
                if targets.len() > 1 {
                    violations.push(format!("{path} extends {targets:?}"));
                }
                if let Some(target) = targets.first() {
                    let base = match target.strip_suffix("[]") {
                        Some(element) if element.len() == 1 => "array".to_string(),
                        Some(element) => format!("{}_array", element.to_snake_case()),
                        None => target.to_snake_case(),
                    };
                    if stem != base
                        && !stem.starts_with(&format!("{base}_"))
                        && !stem.ends_with(&format!("_{base}"))
                    {
                        violations.push(format!(
                            "{path}: extension of {target} must be named `{base}`, \
                             `{base}_<qualifier>`, or `<qualifier>_{base}`"
                        ));
                    }
                }
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
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
