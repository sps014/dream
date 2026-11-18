//! Header-backed C imports are ordinary Dream declarations after source loading.

#[cfg(feature = "native")]
mod api;
#[cfg(feature = "native")]
mod clang;
#[cfg(feature = "native")]
pub(super) use clang::Build;
#[cfg(feature = "native")]
mod model;

use super::source_loader::ProgramAccumulator;
use bumpalo::Bump;
use dream_abi::core_items::C_INTEROP_PACKAGE;
use dream_abi::project::find_project_root;
use dream_abi::target::TargetSpec;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::ImportNode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

pub fn source_scope(path: &Path) -> String {
    let directory = path.parent().unwrap_or_else(|| Path::new(""));
    let root = find_project_root(directory).unwrap_or_else(|| directory.to_owned());
    dunce::canonicalize(&root)
        .unwrap_or(root)
        .to_string_lossy()
        .into_owned()
}

#[derive(Deserialize, Serialize)]
struct Binding {
    fingerprint: String,
    source: String,
    omissions: BTreeMap<String, String>,
}

struct Request {
    source: String,
    import: ImportNode,
}

fn requests(acc: &ProgramAccumulator<'_>) -> Vec<Request> {
    acc.parsed_files
        .iter()
        .flat_map(|(source, program)| {
            program
                .imports
                .iter()
                .filter(|import| import.c.is_some())
                .map(|import| Request {
                    source: source.clone(),
                    import: import.clone(),
                })
        })
        .collect()
}

fn cache_root(entry: &str, target: &TargetSpec) -> PathBuf {
    PathBuf::from(source_scope(Path::new(entry)))
        .join("target/obj/cimport")
        .join(target.llvm_triple())
}

fn attach<'a>(
    request: &Request,
    path: &Path,
    binding: &Binding,
    acc: &mut ProgramAccumulator<'a>,
    arena: &'a Bump,
    diagnostics: &mut DiagnosticBag,
) -> io::Result<()> {
    diagnostics.file_path = Some(request.source.clone());
    let c = request.import.c.as_ref().expect("C import request");
    let mut reported = std::collections::BTreeSet::new();
    for usage in &c.uses {
        if reported.insert(&usage.text)
            && let Some(reason) = binding.omissions.get(&usage.text)
        {
            diagnostics.report_error(
                format!(
                    "C declaration '{}' in {} is unavailable: {reason}",
                    usage.text, request.import.module_name.text
                ),
                Some(usage.position),
            );
        }
    }
    let path = path.to_string_lossy().into_owned();
    if !acc.parsed_files.contains_key(&path) {
        super::generate::merge_generated_file(
            arena,
            acc,
            &path,
            "C header",
            binding.source.clone(),
            diagnostics,
        )?;
        acc.generator_output_files.shift_remove(&path);
        acc.generated_files.shift_remove(&path);
    }
    acc.import_edges
        .entry(request.source.clone())
        .or_default()
        .push(path);
    Ok(())
}

#[cfg(feature = "native")]
pub(super) fn prepare<'a>(
    acc: &mut ProgramAccumulator<'a>,
    arena: &'a Bump,
    diagnostics: &mut DiagnosticBag,
    entry: &str,
    target: &TargetSpec,
    build: clang::Build<'_>,
) -> io::Result<()> {
    let requests = requests(acc);
    if requests.is_empty() {
        return Ok(());
    }
    acc.has_c_imports = true;
    acc.requested_std_packages.insert(C_INTEROP_PACKAGE.into());
    let root = cache_root(entry, target);
    let mut resolved = BTreeMap::new();
    for request in requests {
        let c = request.import.c.as_ref().expect("C import request");
        let stem = c.prefix.trim_end_matches('_');
        let metadata = root.join(format!("{stem}.json"));
        let path = metadata.with_extension("dream");
        let binding = resolved
            .entry(c.prefix.clone())
            .or_insert_with(|| build.generate(&request, &metadata, target));
        match binding {
            Ok(binding) => attach(&request, &path, binding, acc, arena, diagnostics)?,
            Err(error) => {
                diagnostics.file_path = Some(request.source.clone());
                diagnostics.report_error(
                    format!("C import {}: {error}", request.import.module_name.text),
                    Some(request.import.module_name.position),
                );
            }
        }
    }
    Ok(())
}

/// Editor analysis can read compiler output without starting or installing a toolchain.
pub fn replay<'a>(
    acc: &mut ProgramAccumulator<'a>,
    arena: &'a Bump,
    diagnostics: &mut DiagnosticBag,
    entry: &str,
) -> io::Result<()> {
    let root = cache_root(entry, &TargetSpec::host());
    for request in requests(acc) {
        acc.requested_std_packages.insert(C_INTEROP_PACKAGE.into());
        let c = request.import.c.as_ref().expect("C import request");
        let metadata = root.join(format!("{}.json", c.prefix.trim_end_matches('_')));
        if let Some(binding) = std::fs::read(&metadata)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        {
            attach(
                &request,
                &metadata.with_extension("dream"),
                &binding,
                acc,
                arena,
                diagnostics,
            )?;
        } else {
            diagnostics.file_path = Some(request.source.clone());
            diagnostics.report_error(
                "build this project to make the C header bindings available in the editor".into(),
                Some(request.import.module_name.position),
            );
        }
    }
    Ok(())
}
