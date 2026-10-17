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

#[derive(Debug, Clone)]
pub struct GeneratedArtifact {
    pub generator: String,
    pub path: String,
    pub contents: Vec<u8>,
    pub publish: bool,
}

/// A tool output copied next to the compiled program.
#[derive(Debug, Clone)]
pub struct PublishedOutput {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Default)]
pub struct Applied {
    pub replacements: IndexMap<SiteKey, String>,
    pub files: Vec<GeneratedFile>,
    pub artifacts: Vec<GeneratedArtifact>,
    pub published: Vec<PublishedOutput>,
    pub assets: IndexMap<String, (Vec<u8>, Vec<String>)>,
    pub consumed: IndexMap<String, dream_sema::module_graph::ConsumedBy>,
    /// Generators that reported an error; their unexpanded sites are not reported again.
    pub failed: Vec<String>,
}

fn valid_virtual_path(path: &str) -> bool {
    super::artifacts::valid_relative_path(path)
        && path.ends_with(".dream")
        && path.split('/').next() != Some("outputs.json")
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

pub fn apply(
    runs: &[GenRun<'_>],
    root: &std::path::Path,
    diagnostics: &mut DiagnosticBag,
) -> Applied {
    let mut applied = Applied::default();
    let mut publish_owner: IndexMap<String, String> = IndexMap::new();
    let mut file_owner: IndexMap<String, String> = IndexMap::new();
    for run in runs {
        let registered = run.registered;
        let mut failed = false;
        if run.result.version != super::model::SNAPSHOT_VERSION
            || run.result.generator != registered.name
        {
            report(
                diagnostics,
                run,
                true,
                "generator result has an invalid protocol identity".to_string(),
                "",
            );
            applied.failed.push(registered.name.clone());
            continue;
        }
        if let Err(message) = super::transcript::validate(&run.result, &registered.tools) {
            report(diagnostics, run, true, message, "");
            applied.failed.push(registered.name.clone());
            continue;
        }
        for d in &run.result.diagnostics {
            let error = d.severity != "warning";
            failed |= error;
            if !d.artifact.is_empty() {
                if let Some((contents, mappings)) =
                    run.result.outputs.iter().find_map(|o| match o {
                        Output::Artifact {
                            path,
                            contents,
                            mappings,
                            binary: false,
                            ..
                        } if path == &d.artifact && super::artifacts::valid_artifact_path(path) => {
                            Some((contents, mappings))
                        }
                        _ => None,
                    })
                {
                    if let Some(mapping) = super::artifacts::mapping_at(mappings, d.line, d.column)
                    {
                        report(diagnostics, run, error, d.message.clone(), &mapping.target);
                    } else {
                        let path = root
                            .join(&registered.name)
                            .join(&d.artifact)
                            .to_string_lossy()
                            .into_owned();
                        let position = super::artifacts::position(contents, d.line, d.column);
                        let diagnostic = if error {
                            Diagnostic::new(d.message.clone(), position, Some(path))
                        } else {
                            Diagnostic::warning(d.message.clone(), position, Some(path))
                        };
                        diagnostics.report(
                            diagnostic
                                .with_note(format!("reported by generator '{}'", registered.name)),
                        );
                    }
                } else {
                    report(
                        diagnostics,
                        run,
                        true,
                        format!("diagnostic names unknown artifact '{}'", d.artifact),
                        "",
                    );
                    failed = true;
                }
            } else {
                report(diagnostics, run, error, d.message.clone(), &d.target);
            }
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
                Output::Artifact {
                    path,
                    contents,
                    mappings,
                    publish,
                    binary,
                } => {
                    let key = format!("{}/{path}", registered.name);
                    let bytes = super::artifacts::artifact_bytes(contents, *binary);
                    let valid = super::artifacts::valid_artifact_path(path)
                        && bytes.is_some()
                        && (!*binary || mappings.is_empty())
                        && mappings.iter().all(|m| {
                            super::artifacts::valid_mapping(m, contents)
                                && run.spans.get(&m.target).is_some()
                        });
                    if !valid
                        || file_owner.insert(key, registered.name.clone()).is_some()
                        || (*publish
                            && publish_owner
                                .insert(path.clone(), registered.name.clone())
                                .is_some())
                    {
                        report(
                            diagnostics,
                            run,
                            true,
                            format!(
                                "generator '{}' emitted invalid or duplicate artifact '{path}'",
                                registered.name
                            ),
                            "",
                        );
                        failed = true;
                        continue;
                    }
                    applied.artifacts.push(GeneratedArtifact {
                        generator: registered.name.clone(),
                        path: path.clone(),
                        contents: bytes.unwrap_or_default(),
                        publish: *publish,
                    });
                }
                Output::Consume { target, help } => {
                    if let Some((file, span)) = run.spans.consumable.get(target) {
                        if let Some(prior) = applied.consumed.get(target) {
                            report(
                                diagnostics,
                                run,
                                true,
                                format!(
                                    "function '{target}' was already consumed by generator '{}'",
                                    prior.generator
                                ),
                                target,
                            );
                            failed = true;
                        } else {
                            applied.consumed.insert(
                                target.clone(),
                                dream_sema::module_graph::ConsumedBy {
                                    generator: registered.name.clone(),
                                    file: file.clone(),
                                    span: *span,
                                    help: help.clone(),
                                },
                            );
                        }
                    } else {
                        report(
                            diagnostics,
                            run,
                            true,
                            format!(
                                "cannot consume '{target}': expected an unambiguous selected or referenced top-level user function"
                            ),
                            target,
                        );
                        failed = true;
                    }
                }
                Output::ToolRun { .. } => {
                    if !super::artifacts::valid_tool_request(output, &registered.tools) {
                        report(
                            diagnostics,
                            run,
                            true,
                            "invalid request or undeclared generator tool".to_string(),
                            "",
                        );
                        failed = true;
                    }
                }
                Output::Asset {
                    name,
                    source,
                    targets,
                } => {
                    let bytes = source_bytes(&run.result, source);
                    match bytes {
                        Some(Ok(bytes))
                            if !name.is_empty() && !applied.assets.contains_key(name) =>
                        {
                            applied
                                .assets
                                .insert(name.clone(), (bytes, targets.clone()));
                        }
                        _ => {
                            report(
                                diagnostics,
                                run,
                                true,
                                format!("asset '{name}' has an invalid source or duplicate name"),
                                "",
                            );
                            failed = true;
                        }
                    }
                }
                Output::Publish { path, source } => {
                    let bytes = source
                        .starts_with(super::assets::TOOL_SOURCE)
                        .then(|| super::assets::tool_output(&run.result, source))
                        .flatten();
                    match bytes {
                        Some(Ok(bytes))
                            if super::artifacts::valid_artifact_path(path)
                                && publish_owner
                                    .insert(path.clone(), registered.name.clone())
                                    .is_none() =>
                        {
                            applied.published.push(PublishedOutput {
                                path: path.clone(),
                                bytes,
                            });
                        }
                        _ => {
                            report(
                                diagnostics,
                                run,
                                true,
                                format!(
                                    "generator '{}' published '{path}' from an unknown tool output or to an invalid or duplicate path",
                                    registered.name
                                ),
                                "",
                            );
                            failed = true;
                        }
                    }
                }
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

/// An emitted artifact path, or a tool output named `tool:<id>/<path>`.
fn source_bytes(result: &GenResult, source: &str) -> Option<Result<Vec<u8>, String>> {
    if source.starts_with(super::assets::TOOL_SOURCE) {
        return super::assets::tool_output(result, source);
    }
    result.outputs.iter().find_map(|o| match o {
        Output::Artifact {
            path,
            contents,
            binary,
            ..
        } if path == source && super::artifacts::valid_artifact_path(path) => Some(
            super::artifacts::artifact_bytes(contents, *binary)
                .ok_or_else(|| format!("artifact '{path}' is not valid base64")),
        ),
        _ => None,
    })
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
    fn generator(name: &str) -> super::RegisteredGenerator {
        super::RegisteredGenerator {
            name: name.to_string(),
            id: format!("::{name}"),
            module: String::new(),
            file: "generator.dream".into(),
            std_package: None,
            span: None,
            attribute_triggers: Vec::new(),
            call_triggers: Vec::new(),
            syntax_block: false,
            incremental: true,
            tools: Vec::new(),
            entry: None,
        }
    }

    fn run(
        generator: &super::RegisteredGenerator,
        outputs: Vec<super::Output>,
    ) -> super::GenRun<'_> {
        let mut spans = super::IdSpans::default();
        let span = dream_text::text_span::TextSpan {
            start: 10,
            end: 18,
            line_no: 2,
            col_no: 5,
        };
        spans.insert("::selected", Some("main.dream"), span);
        spans
            .consumable
            .insert("::selected".into(), ("main.dream".into(), span));
        super::GenRun {
            registered: generator,
            spans,
            sites: Default::default(),
            result: super::GenResult {
                version: super::super::model::SNAPSHOT_VERSION,
                generator: generator.name.clone(),
                outputs,
                ..Default::default()
            },
        }
    }

    #[test]
    fn binary_artifacts_embed_their_bytes_and_reject_source_maps() {
        let generator = generator("packer");
        let artifact = |path: &str, mappings: Vec<super::super::model::SourceMapping>| {
            super::Output::Artifact {
                path: path.into(),
                contents: "AAEC/w==".into(),
                mappings,
                publish: false,
                binary: true,
            }
        };
        let run = run(
            &generator,
            vec![
                artifact("code.bin", Vec::new()),
                super::Output::Asset {
                    name: "code".into(),
                    source: "code.bin".into(),
                    targets: Vec::new(),
                },
                artifact(
                    "mapped.bin",
                    vec![super::super::model::SourceMapping {
                        line: 1,
                        column: 1,
                        end_line: 1,
                        end_column: 2,
                        target: "::selected".into(),
                    }],
                ),
            ],
        );
        let mut diagnostics = dream_diagnostics::DiagnosticBag::new(None);
        let applied = super::apply(&[run], std::path::Path::new("."), &mut diagnostics);
        assert_eq!(applied.assets["code"].0, vec![0, 1, 2, 255]);
        assert_eq!(applied.artifacts[0].contents, vec![0, 1, 2, 255]);
        assert!(diagnostics.diagnostics.iter().any(|d| {
            d.message
                .contains("invalid or duplicate artifact 'mapped.bin'")
        }));
    }

    #[test]
    fn consume_rights_and_conflicts_are_validated_after_serialization() {
        let first = generator("first");
        let second = generator("second");
        let a = run(
            &first,
            vec![super::Output::Consume {
                target: "::selected".into(),
                help: String::new(),
            }],
        );
        let mut b = run(
            &second,
            vec![
                super::Output::Consume {
                    target: "::selected".into(),
                    help: String::new(),
                },
                super::Output::Consume {
                    target: "system.core::other".into(),
                    help: String::new(),
                },
            ],
        );
        b.result = serde_json::from_str(&serde_json::to_string(&b.result).unwrap()).unwrap();
        let mut diagnostics = dream_diagnostics::DiagnosticBag::new(None);
        let applied = super::apply(&[a, b], std::path::Path::new("generated"), &mut diagnostics);
        assert_eq!(applied.consumed["::selected"].generator, "first");
        assert_eq!(diagnostics.diagnostics.len(), 2);
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| d.message.contains("already consumed"))
        );
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| d.message.contains("cannot consume"))
        );
    }

    #[test]
    fn cached_results_cannot_change_their_generator_identity() {
        let generator = generator("selected");
        let mut run = run(
            &generator,
            vec![super::Output::Consume {
                target: "::selected".into(),
                help: String::new(),
            }],
        );
        run.result.generator = "forged".into();
        let mut diagnostics = dream_diagnostics::DiagnosticBag::new(None);
        let applied = super::apply(&[run], std::path::Path::new("generated"), &mut diagnostics);
        assert!(applied.consumed.is_empty());
        assert!(
            diagnostics.diagnostics[0]
                .message
                .contains("invalid protocol identity")
        );
    }

    #[test]
    fn artifact_errors_map_back_to_the_original_source_node() {
        let generator = generator("mapper");
        let mut run = run(
            &generator,
            vec![super::Output::Artifact {
                path: "source.txt".into(),
                contents: "abc".into(),
                publish: false,
                binary: false,
                mappings: vec![super::super::model::SourceMapping {
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 4,
                    target: "::selected".into(),
                }],
            }],
        );
        run.result
            .diagnostics
            .push(super::super::model::GenDiagnostic {
                severity: "error".into(),
                message: "external error".into(),
                artifact: "source.txt".into(),
                line: 1,
                column: 2,
                ..Default::default()
            });
        let mut diagnostics = dream_diagnostics::DiagnosticBag::new(None);
        super::apply(&[run], std::path::Path::new("generated"), &mut diagnostics);
        assert_eq!(
            diagnostics.diagnostics[0].file_path.as_deref(),
            Some("main.dream")
        );
        assert_eq!(diagnostics.diagnostics[0].span.unwrap().start, 10);
    }
}
