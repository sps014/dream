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
    let npm = if manifest.is_file() {
        crate::driver::project_manifest::ProjectManifest::load(&root)
            .map_err(Error::other)?
            .npm
    } else {
        BTreeMap::new()
    };
    acc.resolution_inputs.insert(manifest);
    acc.resolution_inputs.insert(root.join("dream.npm.lock"));
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
