use std::io::Error;

use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;

use crate::driver::source_loader::ProgramAccumulator;

pub(super) fn prepare<'a>(
    arena: &'a Bump,
    acc: &mut ProgramAccumulator<'a>,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), Error> {
    let packages = dream_stdlib::resolve_packages_to_load(&acc.requested_std_packages);
    if !packages.iter().any(|package| package.name == "system.js") {
        return Ok(());
    }
    let catalog: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tooling/js-bindings/builtins/shared.json"
    ))
    .map_err(Error::other)?;
    let mut files: Vec<super::GeneratedFile> =
        serde_json::from_value(catalog["files"].clone()).map_err(Error::other)?;
    files.sort_by_key(|file| file.name == "globals.dream");
    for generated in files {
        let owner = if generated.name == "globals.dream" {
            "system.js"
        } else {
            "system.js.bindings"
        };
        let path = format!("<std>/generated/{owner}/{}", generated.name);
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
        acc.generator_output_files.shift_remove(&path);
        acc.std_generator_output_files.insert(path);
    }
    Ok(())
}
