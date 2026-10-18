use super::decls::{DeclEntry, DeclIndex, module_of};
use crate::driver::source_loader::ProgramAccumulator;
use dream_syntax::nodes::Visibility;
use indexmap::{IndexMap, IndexSet};

#[derive(Debug, Clone, Default)]
pub(super) struct Scope {
    module: String,
    imports: IndexSet<String>,
    aliases: IndexMap<String, String>,
}

pub(super) fn scopes(acc: &ProgramAccumulator<'_>) -> IndexMap<String, Scope> {
    let prelude: IndexSet<_> = acc
        .file_modules
        .iter()
        .filter(|(file, _)| dream_stdlib::is_std_source(file))
        .map(|(_, module)| module.to_string())
        .collect();
    let mut files: Vec<_> = acc.file_contents.keys().collect();
    files.sort();
    files
        .into_iter()
        .map(|file| {
            let mut scope = Scope {
                module: module_of(acc, Some(file)),
                ..Scope::default()
            };
            // Match the module graph: every loaded prelude package is visible to source files.
            let mut prelude: Vec<_> = prelude.iter().cloned().collect();
            prelude.sort();
            scope.imports.extend(prelude);
            for path in acc.import_edges.get(file).into_iter().flatten() {
                scope.imports.insert(module_of(acc, Some(path)));
            }
            for (module, item, alias, importing) in &acc.aliased_imports {
                if importing == file {
                    scope
                        .aliases
                        .insert(alias.text.clone(), format!("{module}::{item}"));
                }
            }
            (file.clone(), scope)
        })
        .collect()
}

fn std_generator_scopes() -> &'static IndexMap<&'static str, Scope> {
    static SCOPES: std::sync::OnceLock<IndexMap<&'static str, Scope>> = std::sync::OnceLock::new();
    SCOPES.get_or_init(|| {
        let mut scopes = IndexMap::new();
        for owner in dream_stdlib::STD_PACKAGES {
            for package in owner
                .generators
                .iter()
                .filter_map(|name| dream_stdlib::find_package(name))
            {
                for &(file, source) in package.files {
                    let arena = bumpalo::Bump::new();
                    let mut diagnostics = dream_diagnostics::DiagnosticBag::new(None);
                    let lexer = dream_syntax::lexer::Lexer::new(source.to_string());
                    let mut parser =
                        dream_syntax::parser::Parser::new(lexer, &arena, &mut diagnostics);
                    let Ok(ast) = parser.parse() else { continue };
                    let program = ast.get_root();
                    let mut scope = Scope {
                        module: program
                            .module
                            .as_ref()
                            .map(|m| m.path.text.clone())
                            .unwrap_or_default(),
                        ..Default::default()
                    };
                    for import in &program.imports {
                        let module = &import.module_name.text;
                        if let Some(alias) = &import.alias {
                            if let Some((module, item)) = module.rsplit_once('.') {
                                scope
                                    .aliases
                                    .insert(alias.text.clone(), format!("{module}::{item}"));
                            }
                        } else {
                            scope.imports.insert(module.clone());
                        }
                    }
                    scopes.insert(file, scope);
                }
            }
        }
        scopes
    })
}

impl DeclIndex {
    pub(super) fn add_std_generator_scopes(&mut self, acc: &ProgramAccumulator<'_>) {
        let mut prelude: Vec<_> = acc
            .file_modules
            .iter()
            .filter(|(file, _)| dream_stdlib::is_std_source(file))
            .map(|(_, module)| module.to_string())
            .collect();
        prelude.sort();
        prelude.dedup();
        for scope in self.scopes.values_mut() {
            scope.imports.extend(prelude.iter().cloned());
        }
        let cached = std_generator_scopes();
        for owner in dream_stdlib::resolve_packages_to_load(&acc.requested_std_packages) {
            for package in owner
                .generators
                .iter()
                .filter_map(|name| dream_stdlib::find_package(name))
            {
                for &(file, _) in package.files {
                    if let Some(scope) = cached.get(file) {
                        let mut scope = scope.clone();
                        scope.imports.extend(prelude.iter().cloned());
                        self.scopes.insert(file.to_string(), scope);
                    }
                }
            }
        }
    }

    /// DeclarationPath is resolved in its source file, before import aliases disappear from snapshots.
    pub(super) fn attribute_reference(&self, path: &str, file: Option<&str>) -> Option<String> {
        let scope = self.scopes.get(file?)?;
        let parts: Vec<_> = path.split('.').collect();
        let visible = |entry: &DeclEntry| {
            if entry.visibility == Visibility::Private {
                entry.file.as_deref() == file
            } else if entry.visibility == Visibility::Internal {
                entry.file.as_deref() == file
                    || (!scope.module.is_empty() && entry.module == scope.module)
                    || (entry
                        .file
                        .as_deref()
                        .is_some_and(dream_stdlib::is_std_source)
                        && file.is_some_and(dream_stdlib::is_std_source))
            } else {
                entry.module == scope.module
                    || scope.imports.contains(&entry.module)
                    || scope.aliases.values().any(|id| id == &entry.id)
            }
        };
        let lookup = |base: &str, member: Option<&str>| {
            let entry = self
                .by_name
                .get(base)
                .or_else(|| self.functions.get(base))?;
            if !visible(entry) {
                return None;
            }
            match member {
                Some(member) => entry
                    .members
                    .iter()
                    .any(|m| m == member)
                    .then(|| format!("{base}.{member}")),
                None => Some(base.to_string()),
            }
        };
        if let Some(base) = scope.aliases.get(parts[0]) {
            return if parts.len() <= 2 {
                lookup(base, parts.get(1).copied())
            } else {
                None
            };
        }
        if parts.len() <= 2 {
            let name = parts[0];
            let member = parts.get(1).copied();
            if let Some(found) = lookup(&format!("{}::{name}", scope.module), member) {
                return Some(found);
            }
            let mut found = None;
            for module in &scope.imports {
                if let Some(candidate) = lookup(&format!("{module}::{name}"), member) {
                    if found.is_some() {
                        return None;
                    }
                    found = Some(candidate);
                }
            }
            if found.is_some() {
                return found;
            }
        }
        for split in 1..parts.len() {
            let rest = &parts[split + 1..];
            if rest.len() <= 1 {
                let base = format!("{}::{}", parts[..split].join("."), parts[split]);
                if let Some(found) = lookup(&base, rest.first().copied()) {
                    return Some(found);
                }
            }
        }
        None
    }
}

/// Generated imports bind canonical references without reloading source or expanding the graph.
pub(super) fn merge_imports(
    acc: &mut ProgramAccumulator<'_>,
    program: &dream_syntax::nodes::ProgramNode<'_>,
    file: &str,
    diagnostics: &mut dream_diagnostics::DiagnosticBag,
) {
    let loaded = dream_stdlib::resolve_packages_to_load(&acc.requested_std_packages);
    let declarations = DeclIndex::build(acc);
    for import in &program.imports {
        let name = import.module_name.text.replace('/', ".");
        let (module, item) = if import.alias.is_some() {
            match name.rsplit_once('.') {
                Some((module, item)) => (module, Some(item)),
                None => (name.as_str(), None),
            }
        } else {
            (name.as_str(), None)
        };
        let mut paths: Vec<_> = acc
            .file_modules
            .iter()
            .filter(|(_, m)| m.as_ref() == module)
            .map(|(path, _)| path.clone())
            .collect();
        let std_loaded = loaded.iter().any(|p| p.name == module);
        let target_exists = item.is_none_or(|item| {
            acc.all_functions
                .iter()
                .any(|f| f.name.text == item && module_of(acc, f.file_path.as_deref()) == module)
                || declarations
                    .type_decl(&format!("{module}::{item}"))
                    .is_some()
        });
        if (!std_loaded && paths.is_empty()) || !target_exists {
            diagnostics.report(dream_diagnostics::Diagnostic::new(
                format!("generated file imports '{name}'; generated code may only import modules the program already imports"),
                Some(import.module_name.position), Some(file.to_string()),
            ));
            continue;
        }
        paths.sort();
        acc.import_edges
            .entry(file.to_string())
            .or_default()
            .extend(paths);
        if let (Some(alias), Some(item)) = (&import.alias, item) {
            acc.aliased_imports.push((
                module.to_string(),
                item.to_string(),
                alias.clone(),
                file.to_string(),
            ));
        }
    }
}
