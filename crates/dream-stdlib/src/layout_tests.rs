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

#[test]
fn runtime_host_fields_name_one_known_capability() {
    use dream_abi::host_capability::HostCapability;
    let fields = crate::runtime_host_fields();
    assert!(!fields.is_empty());
    for pair in fields.windows(2) {
        assert_ne!(pair[0].0, pair[1].0, "{} declares two hosts", pair[0].0);
    }
    for (field, capability) in &fields {
        assert!(
            HostCapability::from_name(capability).is_some(),
            "{field}: unknown host '{capability}'"
        );
    }
}

#[test]
fn compiler_does_not_name_library_packages() {
    let library: Vec<&str> = crate::STD_PACKAGES
        .iter()
        .filter(|p| !p.core_language)
        .map(|p| p.name)
        .collect();
    let library_paths: std::collections::BTreeSet<&str> = crate::STD_PACKAGES
        .iter()
        .filter(|p| !p.core_language)
        .flat_map(|p| p.files.iter().map(|(path, _)| *path))
        .collect();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let roots = [
        "crates/dream-syntax/src",
        "crates/dream-types/src",
        "crates/dream-hir/src",
        "crates/dream-abi/src",
        "crates/dream-sema/src",
        "crates/dream-mir/src",
        "src/driver",
        "src/execution",
        "tooling/dream-lsp/src",
    ];
    let mut violations = Vec::new();
    for rel in roots {
        let dir = root.join(rel);
        walk_rust(&dir, &mut |path| {
            if path.ends_with("core_items.rs") {
                return;
            }
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let text = strip_cfg_test(&text);
            for literal in rust_string_literals(&text) {
                if library.contains(&literal.as_str())
                    || library_paths.contains(literal.as_str())
                    || library
                        .iter()
                        .any(|pkg| literal.starts_with(&format!("{pkg}::")))
                {
                    violations.push(format!(
                        "{}: `{literal}`",
                        path.strip_prefix(&root).unwrap_or(path).display()
                    ));
                }
            }
        });
    }
    violations.sort();
    assert!(
        violations.is_empty(),
        "compiler names a library package:\n{}",
        violations.join("\n")
    );
}

fn walk_rust(dir: &std::path::Path, visit: &mut dyn FnMut(&std::path::Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            walk_rust(&path, visit);
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
            visit(&path);
        }
    }
}

fn strip_cfg_test(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..at]);
        let after = &rest[at + "#[cfg(test)]".len()..];
        let trimmed = after.trim_start();
        let skipped = after.len() - trimmed.len();
        if (trimmed.starts_with("mod ")
            || trimmed.starts_with("fn ")
            || trimmed.starts_with("impl "))
            && let Some(rel) = matching_brace_end(trimmed)
        {
            rest = &trimmed[rel..];
            continue;
        }
        rest = &after[skipped..];
    }
    out.push_str(rest);
    out
}

fn matching_brace_end(text: &str) -> Option<usize> {
    let start = text.find('{')?;
    let bytes = text.as_bytes();
    let mut depth = 0;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn rust_string_literals(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let mut j = i + 1;
            let mut lit = String::new();
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'"' {
                    break;
                }
                lit.push(bytes[j] as char);
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'"' && !lit.is_empty() {
                out.push(lit);
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    out
}
