//! Validates generator results (fresh or replayed alike) into site replacements, generated
//! files and diagnostics anchored at the snapshot identities they name.

use super::decls::IdSpans;
use super::model::{GenResult, Output};
use super::registry::RegisteredGenerator;
use super::sites::SiteKey;
use dream_diagnostics::{Diagnostic, DiagnosticBag};
use indexmap::IndexMap;

/// A generator to produce a result for, with its serialized snapshot.
pub struct Job<'g> {
    pub registered: &'g RegisteredGenerator,
    pub built: super::snapshot::BuiltSnapshot,
    /// The snapshot as the generator executable reads it.
    #[cfg(feature = "native")]
    pub json: String,
}

/// One generator's result with the maps from its snapshot.
pub struct GenRun<'g> {
    pub registered: &'g RegisteredGenerator,
    pub spans: IdSpans,
    pub sites: IndexMap<String, SiteKey>,
    pub result: GenResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub generator: String,
    /// Generator-relative virtual path (`models.json.dream`).
    pub path: String,
    pub source: String,
}

#[derive(Default)]
pub struct Applied {
    pub replacements: IndexMap<SiteKey, String>,
    pub files: Vec<GeneratedFile>,
    /// Generators that reported an error; their unexpanded sites are not reported again.
    pub failed: Vec<String>,
}

fn valid_virtual_path(path: &str) -> bool {
    let p = std::path::Path::new(path);
    !path.is_empty()
        && path.ends_with(".dream")
        && !path.contains('\\')
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

fn report(
    diagnostics: &mut DiagnosticBag,
    run: &GenRun<'_>,
    error: bool,
    message: String,
    target: &str,
) {
    let registered = run.registered;
    let (file, span) = match run.spans.get(target) {
        Some((file, span)) => (file.map(str::to_string), Some(span)),
        None => (Some(registered.file.clone()), registered.span),
    };
    let note = format!("reported by generator '{}'", registered.name);
    let diagnostic = if error {
        Diagnostic::new(message, span, file)
    } else {
        Diagnostic::warning(message, span, file)
    };
    diagnostics.report(diagnostic.with_note(note));
}

pub fn apply(runs: &[GenRun<'_>], diagnostics: &mut DiagnosticBag) -> Applied {
    let mut applied = Applied::default();
    let mut file_owner: IndexMap<String, String> = IndexMap::new();
    for run in runs {
        let registered = run.registered;
        let mut failed = false;
        for d in &run.result.diagnostics {
            let error = d.severity != "warning";
            failed |= error;
            report(diagnostics, run, error, d.message.clone(), &d.target);
        }
        for output in &run.result.outputs {
            match output {
                Output::Replace { site, source } => match run.sites.get(site) {
                    Some(key) => {
                        if applied
                            .replacements
                            .insert(key.clone(), source.clone())
                            .is_some()
                        {
                            report(
                                diagnostics,
                                run,
                                true,
                                format!(
                                    "generator '{}' replaced syntax block '{site}' twice",
                                    registered.name
                                ),
                                site,
                            );
                            failed = true;
                        }
                    }
                    None => {
                        report(
                            diagnostics,
                            run,
                            true,
                            format!(
                                "generator '{}' replaced unknown syntax block '{site}'",
                                registered.name
                            ),
                            "",
                        );
                        failed = true;
                    }
                },
                Output::File { path, source } => {
                    if !valid_virtual_path(path) {
                        report(
                            diagnostics,
                            run,
                            true,
                            format!(
                                "generator '{}' emitted file '{path}'; generated paths must be relative '.dream' paths without '..'",
                                registered.name
                            ),
                            "",
                        );
                        failed = true;
                        continue;
                    }
                    let key = format!("{}/{path}", registered.name);
                    if file_owner.insert(key, registered.name.clone()).is_some() {
                        report(
                            diagnostics,
                            run,
                            true,
                            format!("generator '{}' emitted '{path}' twice", registered.name),
                            "",
                        );
                        failed = true;
                        continue;
                    }
                    applied.files.push(GeneratedFile {
                        generator: registered.name.clone(),
                        path: path.clone(),
                        source: source.clone(),
                    });
                }
            }
        }
        if failed {
            applied.failed.push(registered.name.clone());
        }
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::valid_virtual_path;

    #[test]
    fn generated_paths_stay_inside_the_generator_folder() {
        assert!(valid_virtual_path("models.json.dream"));
        assert!(valid_virtual_path("nested/a.dream"));
        assert!(!valid_virtual_path("../escape.dream"));
        assert!(!valid_virtual_path("/abs.dream"));
        assert!(!valid_virtual_path("a.txt"));
        assert!(!valid_virtual_path("a\\b.dream"));
        assert!(!valid_virtual_path(""));
    }
}
