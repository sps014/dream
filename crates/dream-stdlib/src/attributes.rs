//! The IDE catalog is derived from the same embedded declarations loaded by the compiler.

use dream_abi::attributes::{UserAttribute, UserAttributes};
use std::sync::OnceLock;

pub fn attribute_catalog() -> &'static UserAttributes {
    static CATALOG: OnceLock<UserAttributes> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let arena = bumpalo::Bump::new();
        let mut diagnostics = dream_diagnostics::DiagnosticBag::new(None);
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut modules = std::collections::BTreeMap::new();
        for package in crate::STD_PACKAGES {
            for &(path, source) in package.files {
                if !source.contains("@attribute") && !source.contains("public enum ") {
                    continue;
                }
                let mut parser = dream_syntax::parser::Parser::new(
                    dream_syntax::lexer::Lexer::new(source.to_string()),
                    &arena,
                    &mut diagnostics,
                );
                if let Ok(ast) = parser.parse() {
                    let program = ast.get_root();
                    modules.insert(path.to_string(), package.name.to_string());
                    for s in &program.structs {
                        let mut s = s.clone();
                        s.file_path = Some(path.into());
                        structs.push(s);
                    }
                    for e in &program.enums {
                        let mut e = e.clone();
                        e.file_path = Some(path.into());
                        enums.push(e);
                    }
                }
            }
        }
        let module_of = |file: Option<&str>| {
            file.and_then(|f| modules.get(f))
                .cloned()
                .unwrap_or_default()
        };
        UserAttributes::collect(
            &structs,
            &enums,
            &module_of,
            Default::default(),
            &mut diagnostics,
        )
    })
}

pub fn all_specs() -> impl Iterator<Item = &'static UserAttribute> {
    attribute_catalog().iter()
}

pub fn find_spec(name: &str) -> Option<&'static UserAttribute> {
    attribute_catalog().get(name)
}

/// Parse-only discovery may identify compiler markers, but compilation still validates the full registry.
pub fn resolve_source_attribute_ids(program: &dream_syntax::nodes::ProgramNode<'_>) {
    let module = program
        .module
        .as_ref()
        .map(|m| m.path.text.as_str())
        .unwrap_or("");
    let imported: Vec<_> = program
        .imports
        .iter()
        .map(|i| i.module_name.text.replace('/', "."))
        .collect();
    let resolve = |attrs: &[dream_syntax::nodes::AttributeNode]| {
        for attr in attrs {
            let Some(decl) = attribute_catalog().get(&attr.name.text) else {
                continue;
            };
            if program
                .structs
                .iter()
                .any(|s| s.name.text == attr.name.text)
            {
                continue;
            }
            if decl.module == "system.core"
                || decl.module == module
                || imported.contains(&decl.module)
            {
                let _ = attr.resolved_id.set(decl.id.clone());
            }
        }
    };
    let functions = |fs: &[dream_syntax::nodes::FunctionNode<'_>]| {
        for f in fs {
            resolve(&f.attributes);
            for p in &f.parameters {
                resolve(&p.attributes);
            }
        }
    };
    functions(&program.functions);
    for s in &program.structs {
        resolve(&s.attributes);
        functions(&s.methods);
        for f in &s.fields {
            resolve(&f.attributes);
        }
    }
    for e in &program.enums {
        resolve(&e.attributes);
        functions(&e.methods);
        for v in &e.variants {
            for f in &v.fields {
                resolve(&f.attributes);
            }
        }
    }
    for e in &program.extends {
        functions(&e.methods);
    }
}
