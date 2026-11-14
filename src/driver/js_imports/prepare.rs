use super::*;
use crate::driver::source_loader::ProgramAccumulator;
use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;

fn root(entry: &str) -> PathBuf {
    let parent = Path::new(entry)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    dream_abi::project::find_project_root(parent).unwrap_or_else(|| parent.to_path_buf())
}

/// Adds the `[npm]` tables of dependency packages whose files contain a `jsimport`, so a package
/// can depend on an npm module without every program repeating it. One npm package must resolve
/// to one version across the program.
fn merge_dependency_npm(
    root: &Path,
    imports: &[(String, dream_syntax::nodes::ImportNode)],
    npm: &mut BTreeMap<String, String>,
    acc: &mut ProgramAccumulator<'_>,
) -> Result<(), String> {
    let mut owners: BTreeMap<String, PathBuf> = npm
        .keys()
        .map(|name| (name.clone(), root.join("dream.toml")))
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    for (file, _) in imports {
        let Some(package) = Path::new(file)
            .parent()
            .and_then(dream_abi::project::find_project_root)
        else {
            continue;
        };
        let package = dunce::canonicalize(&package).unwrap_or(package);
        if dunce::canonicalize(root).is_ok_and(|r| r == package) || !seen.insert(package.clone()) {
            continue;
        }
        let manifest_path = package.join("dream.toml");
        acc.resolution_inputs.insert(manifest_path.clone());
        let manifest = crate::driver::project_manifest::ProjectManifest::load(&package)?;
        for (name, version) in manifest.npm {
            match npm.get(&name) {
                Some(existing) if existing != &version => {
                    return Err(format!(
                        "npm package '{name}' is required as '{existing}' by {} and as '{version}' by {}",
                        owners[&name].display(),
                        manifest_path.display()
                    ));
                }
                Some(_) => {}
                None => {
                    owners.insert(name.clone(), manifest_path.clone());
                    npm.insert(name, version);
                }
            }
        }
    }
    Ok(())
}

/// Expands JS imports before prelude/attribute preparation, shared by builds and editor analysis.
pub fn prepare<'a>(
    arena: &'a Bump,
    acc: &mut ProgramAccumulator<'a>,
    entry: &str,
    target: &str,
    runtime: bool,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), Error> {
    let imports: Vec<_> = acc
        .parsed_files
        .iter()
        .flat_map(|(file, program)| {
            program
                .imports
                .iter()
                .filter(|import| import.is_js)
                .map(|import| (file.clone(), import.clone()))
        })
        .collect();
    builtins::prepare(arena, acc, diagnostics)?;
    if imports.is_empty() {
        return Ok(());
    }
    acc.requested_std_packages.insert("system.js".into());
    let root = std::path::absolute(root(entry))?;
    acc.js_project_root = Some(root.clone());
    let manifest = root.join("dream.toml");
    let mut npm = if manifest.is_file() {
        crate::driver::project_manifest::ProjectManifest::load(&root)
            .map_err(Error::other)?
            .npm
    } else {
        BTreeMap::new()
    };
    acc.resolution_inputs.insert(manifest);
    acc.resolution_inputs.insert(root.join("dream.npm.lock"));
    if let Err(message) = merge_dependency_npm(&root, &imports, &mut npm, acc) {
        diagnostics.report_error(message, None);
        return Ok(());
    }
    let mut requested = Vec::new();
    for (file, import) in &imports {
        let specifier: String = match serde_json::from_str(&import.module_name.text) {
            Ok(specifier) => specifier,
            Err(_) => {
                diagnostics.file_path = Some(file.clone());
                diagnostics.report_error(
                    "jsimport requires a valid quoted module specifier".into(),
                    Some(import.module_name.position),
                );
                continue;
            }
        };
        let origin = if !Path::new(file).is_absolute() {
            Path::new(entry)
                .parent()
                .unwrap_or(Path::new("."))
                .join(file)
        } else {
            PathBuf::from(file)
        };
        requested.push(
            serde_json::json!({"specifier": specifier, "file": std::path::absolute(origin)?}),
        );
    }
    if requested.len() != imports.len() {
        return Ok(());
    }
    let request = serde_json::json!({"mode": "inspect", "root": root, "npm": npm, "imports": requested, "target": target, "runtime": runtime});
    let result = match cache::run(&root, &request) {
        Ok(result) => result,
        Err(error) => {
            for (file, import) in &imports {
                diagnostics.file_path = Some(file.clone());
                diagnostics.report_error(
                    format!("JavaScript binding generation: {error}"),
                    Some(import.module_name.position),
                );
            }
            return Ok(());
        }
    };
    if let Some(code) = result["bundle"]["code"].as_str() {
        acc.js_bundles.insert(target.into(), code.into());
    }
    let modules: Vec<GeneratedModule> =
        serde_json::from_value(result["modules"].clone()).map_err(Error::other)?;
    for ((file, import), mut module) in imports.into_iter().zip(modules) {
        module
            .files
            .sort_by_key(|source| source.name == "bindings.dream");
        acc.resolution_inputs.extend(module.inputs);
        acc.js_modules.push(JsModule {
            id: module.id.clone(),
            resolved: module.resolved,
        });
        for generated in module.files {
            let path = if generated.name == "bindings.dream" {
                format!("<js>/{}/{}", module.id, generated.name)
            } else {
                format!("<js>/types/{}", generated.name)
            };
            if acc.parsed_files.contains_key(&path) {
                continue;
            }
            crate::driver::generate::merge_generated_file(
                arena,
                acc,
                &path,
                "system.js.bindings",
                generated.source,
                diagnostics,
            )?;
            // These wrappers are compiler-owned, like native shims, rather than user generators.
            acc.generator_output_files.shift_remove(&path);
            acc.import_edges.entry(file.clone()).or_default().push(path);
        }
        diagnostics.file_path = Some(file.clone());
        if !module.warnings.is_empty() {
            let report = module.warnings.join("\n");
            acc.published_artifacts.insert(
                format!("js-bindings/{}.coverage.txt", module.id),
                report.into_bytes(),
            );
        }
        if let Some(alias) = &import.alias {
            acc.aliased_imports.push((
                module.namespace.clone(),
                module.root_name.clone(),
                alias.clone(),
                file.clone(),
            ));
            if let Some(program) = acc.parsed_files.get_mut(&file)
                && let Some(original) = program
                    .imports
                    .iter_mut()
                    .find(|i| i.is_js && i.module_name.position == import.module_name.position)
            {
                original.module_name.text = format!("{}.{}", module.namespace, module.root_name);
                original.is_js = false;
            }
        }
    }
    Ok(())
}
