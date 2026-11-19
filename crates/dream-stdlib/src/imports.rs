//! Which opt-in std package declares a public name, so a diagnostic can suggest the missing
//! `import`. Built from the same embedded sources the compiler loads.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use dream_syntax::nodes::Visibility;

#[derive(Default)]
struct ImportIndex {
    names: BTreeMap<String, &'static str>,
    members: BTreeMap<(String, String), &'static str>,
}

fn index() -> &'static ImportIndex {
    static INDEX: OnceLock<ImportIndex> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = ImportIndex::default();
        for package in crate::STD_PACKAGES {
            if crate::BOOTSTRAP_PACKAGES.contains(&package.name) {
                continue;
            }
            for &(_, source) in package.files {
                let arena = bumpalo::Bump::new();
                let mut diagnostics = dream_diagnostics::DiagnosticBag::new(None);
                let mut parser = dream_syntax::parser::Parser::new(
                    dream_syntax::lexer::Lexer::new(source.to_string()),
                    &arena,
                    &mut diagnostics,
                );
                let ast = parser.parse();
                let program = ast.get_root();
                let public = |visibility: Visibility| visibility == Visibility::Public;
                let names = program
                    .structs
                    .iter()
                    .filter(|s| public(s.visibility))
                    .map(|s| &s.name.text)
                    .chain(
                        program
                            .interfaces
                            .iter()
                            .filter(|i| public(i.visibility))
                            .map(|i| &i.name.text),
                    )
                    .chain(
                        program
                            .enums
                            .iter()
                            .filter(|e| public(e.visibility))
                            .map(|e| &e.name.text),
                    )
                    .chain(
                        program
                            .functions
                            .iter()
                            .filter(|f| public(f.visibility))
                            .map(|f| &f.name.text),
                    );
                for name in names {
                    index.names.entry(name.clone()).or_insert(package.name);
                }
                for extend in &program.extends {
                    for method in extend.methods.iter().filter(|m| public(m.visibility)) {
                        index
                            .members
                            .entry((extend.target.text.clone(), method.name.text.clone()))
                            .or_insert(package.name);
                    }
                }
            }
        }
        index
    })
}

/// The opt-in package that declares the public top-level type or function `name`.
pub fn package_exporting(name: &str) -> Option<&'static str> {
    index().names.get(name).copied()
}

/// The opt-in package whose `extend target { … }` block adds the public method `member`.
pub fn package_extending(target: &str, member: &str) -> Option<&'static str> {
    index()
        .members
        .get(&(target.to_string(), member.to_string()))
        .copied()
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_packages_for_types_and_extension_methods() {
        assert_eq!(
            super::package_extending("System", "println"),
            Some("system")
        );
        assert_eq!(super::package_exporting("Regex"), Some("system.text"));
        assert_eq!(super::package_exporting("System"), None);
    }
}
