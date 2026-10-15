//! The generate pass: discover generators, skip everything when no trigger matches, otherwise
//! snapshot → replay or run → validate → materialize → merge.

use super::call_sites::{FoundCall, collect_calls};
use super::decls::DeclIndex;
use super::paths::ProjectPaths;
use super::registry::{CallTrigger, RegisteredGenerator, Registration};
use super::sites::{Site, collect_sites, report_unexpanded};
use super::snapshot::{BuiltSnapshot, ProgramFacts, has_inputs, program_carries};
use super::snapshot_expand::build_snapshot;
use super::stage::GeneratorStage;
use super::stats::{Event, record};
use crate::driver::source_loader::ProgramAccumulator;
use crate::driver::toolchain::ToolchainConfig;
use bumpalo::Bump;
use dream_abi::attributes::UserAttributes;
use dream_diagnostics::{Diagnostic, DiagnosticBag};
use std::io::Error;
use std::sync::Arc;

pub struct GenerateRequest<'r> {
    pub config: &'r Arc<ToolchainConfig>,
    pub stage: GeneratorStage,
    pub entry_file: &'r str,
    pub target: &'r dream_abi::target::TargetSpec,
    /// Use the files the last build wrote under `.dream/generated` instead of running anything,
    /// and leave them untouched (the language server, which must not race a build).
    pub replay_materialized: bool,
}

/// Everything the pass derives from the program before deciding what to run.
pub struct PassInputs {
    pub gens: Vec<RegisteredGenerator>,
    pub index: DeclIndex,
    #[cfg(feature = "native")]
    pub config: Arc<ToolchainConfig>,
    pub source_spans: indexmap::IndexMap<String, Vec<dream_syntax::nodes::program::SourceNodeSpan>>,
    pub semantic: dream_sema::gen_facts::GenFacts,
    pub sites: Vec<Site>,
    pub call_triggers: Vec<CallTrigger>,
    pub calls: Vec<FoundCall>,
    pub paths: ProjectPaths,
    pub target: String,
}

pub fn gather(
    req: &GenerateRequest<'_>,
    acc: &ProgramAccumulator<'_>,
    attributes: &UserAttributes,
    diagnostics: &mut DiagnosticBag,
) -> PassInputs {
    let manifest = if req.stage == GeneratorStage::All {
        super::manifest::load_manifest_generators(req.entry_file)
    } else {
        Vec::new()
    };
    let index = DeclIndex::build(acc);
    let gens = Registration {
        index: &index,
        attributes,
        stage: req.stage,
    }
    .discover(acc, &manifest, diagnostics);
    let mut call_triggers: Vec<CallTrigger> = Vec::new();
    for t in gens.iter().flat_map(|g| &g.call_triggers) {
        if !call_triggers.contains(t) {
            call_triggers.push(t.clone());
        }
    }
    let sites = collect_sites(acc);
    let calls = collect_calls(acc, &call_triggers, None);
    let applies = gens.iter().any(|g| {
        (g.syntax_block && sites.iter().any(|s| s.name == g.name))
            || g.call_triggers
                .iter()
                .any(|t| calls.iter().any(|c| call_triggers[c.trigger].id == t.id))
            || program_carries(acc, &g.attribute_triggers, attributes)
    });
    let semantic = if applies {
        let graph = acc.module_graph();
        let arena = Bump::new();
        dream_sema::analyzer::Analyzer::new(&graph, &arena)
            .with_aliased_imports(acc.aliased_imports.clone())
            .with_target_layout(dream_hir::TargetLayout {
                ptr_size: req.target.ptr_size,
                ptr_align: req.target.ptr_align,
            })
            .analyze_for_generators()
    } else {
        Default::default()
    };
    let calls = if applies {
        collect_calls(acc, &call_triggers, Some(&semantic))
    } else {
        calls
    };
    PassInputs {
        sites,
        calls,
        semantic,
        source_spans: acc
            .parsed_files
            .iter()
            .map(|(path, p)| (path.clone(), p.node_spans.clone()))
            .collect(),
        #[cfg(feature = "native")]
        config: req.config.clone(),
        call_triggers,
        gens,
        index,
        paths: ProjectPaths::for_entry(req.entry_file),
        target: req.target.triple.to_string(),
    }
}

impl PassInputs {
    /// Whether any of the generator's triggers fires in this program.
    pub fn applicable(
        &self,
        registered: &RegisteredGenerator,
        acc: &ProgramAccumulator<'_>,
        attributes: &UserAttributes,
    ) -> bool {
        (registered.syntax_block && self.sites.iter().any(|s| s.name == registered.name))
            || self.calls.iter().any(|c| {
                let id = &self.call_triggers[c.trigger].id;
                registered.call_triggers.iter().any(|t| &t.id == id)
            })
            || program_carries(acc, &registered.attribute_triggers, attributes)
    }

    pub fn snapshot(
        &self,
        acc: &ProgramAccumulator<'_>,
        attributes: &UserAttributes,
        registered: &RegisteredGenerator,
    ) -> Result<BuiltSnapshot, String> {
        let facts = ProgramFacts {
            index: &self.index,
            attributes,
            sites: &self.sites,
            call_triggers: &self.call_triggers,
            calls: &self.calls,
            paths: &self.paths,
            target: &self.target,
            semantic: &self.semantic,
            source_spans: &self.source_spans,
            sources: &acc.file_contents,
            #[cfg(feature = "native")]
            config: &self.config,
        };
        build_snapshot(acc, &facts, registered)
    }

    pub fn generated_root(&self, config: &ToolchainConfig, entry_file: &str) -> std::path::PathBuf {
        super::materialize::entry_dir(
            &self.paths.generated_dir(&config.generator_cache_root()),
            entry_file,
        )
    }

    fn report_sites(
        &self,
        done: &dyn Fn(&Site) -> bool,
        failed: &[String],
        consumed: &[(String, String, dream_text::text_span::TextSpan)],
        diagnostics: &mut DiagnosticBag,
    ) {
        let claimed = |name: &str| self.gens.iter().any(|g| g.syntax_block && g.name == name);
        let sites: Vec<_> = self
            .sites
            .iter()
            .filter(|site| {
                !consumed.iter().any(|(_, file, span)| {
                    site.key.0 == *file
                        && span.start <= site.name_span.start
                        && site.name_span.start < span.end
                })
            })
            .cloned()
            .collect();
        report_unexpanded(&sites, &claimed, diagnostics);
        for site in &sites {
            if claimed(&site.name) && !done(site) && !failed.contains(&site.name) {
                diagnostics.report(Diagnostic::new(
                    format!(
                        "generator '{}' did not expand this syntax block (call ctx.replace(block, ...))",
                        site.name
                    ),
                    Some(site.name_span),
                    Some(site.key.0.clone()).filter(|f| !f.is_empty()),
                ));
            }
        }
    }
}

pub fn run_generators<'a>(
    req: &GenerateRequest<'_>,
    arena: &'a Bump,
    acc: &mut ProgramAccumulator<'a>,
    attributes: &UserAttributes,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), Error> {
    record(Event::Pass);
    let started = std::time::Instant::now();
    let inputs = gather(req, acc, attributes, diagnostics);
    let mut applicable: Vec<&RegisteredGenerator> = Vec::new();
    for g in &inputs.gens {
        if inputs.applicable(g, acc, attributes) {
            applicable.push(g);
        } else {
            tracing::info!("gen {}: skipped", g.name);
        }
    }
    if applicable.is_empty() {
        record(Event::FastPathSkip);
        inputs.report_sites(&|_| false, &[], &[], diagnostics);
        return Ok(());
    }
    let mut jobs = Vec::new();
    for registered in applicable {
        match inputs.snapshot(acc, attributes, registered) {
            Ok(built) if has_inputs(&built.snapshot) => {
                acc.untracked_generator_inputs |= !registered.incremental;
                for file in &built.snapshot.additional_files {
                    acc.resolution_inputs
                        .insert(inputs.paths.root.join(&file.path));
                }
                #[cfg(feature = "native")]
                let job = {
                    let json = if req.replay_materialized {
                        String::new()
                    } else {
                        serde_json::to_string(&built.snapshot)
                            .map_err(|e| Error::other(format!("generator snapshot: {e}")))?
                    };
                    super::apply::Job {
                        registered,
                        built,
                        json,
                    }
                };
                #[cfg(not(feature = "native"))]
                let job = super::apply::Job { registered, built };
                jobs.push(job);
            }
            Ok(_) => {}
            Err(message) => diagnostics.report(Diagnostic::new(
                message,
                registered.span,
                Some(registered.file.clone()),
            )),
        }
    }
    let snapshotted = started.elapsed();
    let root = inputs.generated_root(req.config, req.entry_file);
    let (runs, mut failed) = if req.replay_materialized || !cfg!(feature = "native") {
        replay_materialized(&root, jobs)
    } else {
        run_jobs(req.config, jobs, diagnostics)
    };
    #[cfg(feature = "native")]
    let runs = if !req.replay_materialized {
        super::consumption::stabilize(
            req,
            &inputs,
            acc,
            attributes,
            runs,
            &mut failed,
            diagnostics,
        )
    } else {
        runs
    };
    let consumed_ranges = super::consumption::ranges(&runs);
    let applied = super::apply::apply(&runs, &root, diagnostics);
    failed.extend(applied.failed.iter().cloned());
    for artifact in &applied.artifacts {
        if artifact.publish {
            acc.published_artifacts
                .insert(artifact.path.clone(), artifact.contents.clone());
        }
        acc.file_contents.insert(
            root.join(&artifact.generator)
                .join(&artifact.path)
                .to_string_lossy()
                .into_owned(),
            artifact.contents.clone(),
        );
    }
    for (name, (bytes, targets)) in &applied.assets {
        let family = if req.target.capabilities.linear_memory {
            "wasm"
        } else {
            "native"
        };
        if targets.is_empty()
            || targets
                .iter()
                .any(|t| t == family || t == &req.target.triple.to_string())
        {
            acc.embedded_assets.insert(name.clone(), bytes.clone());
        }
    }
    acc.all_functions.retain(|f| {
        let module = super::decls::module_of_files(&acc.file_modules, f.file_path.as_deref());
        !applied
            .consumed
            .contains_key(&dream_abi::attributes::decl_identity(&module, &f.name.text))
    });
    acc.consumed.extend(
        applied
            .consumed
            .iter()
            .map(|(id, by)| (id.clone(), by.clone())),
    );
    let ran: Vec<String> = runs.iter().map(|r| r.registered.name.clone()).collect();
    let files = if req.replay_materialized || !cfg!(feature = "native") {
        super::materialize::located(&root, &applied.files)
    } else {
        super::materialize::materialize(&root, &runs, &applied.files, &applied.artifacts)
    };
    for ((path, text), generated) in files.into_iter().zip(&applied.files) {
        acc.resolution_inputs.insert(path.clone());
        super::merge::merge_generated_file(
            arena,
            acc,
            &path.to_string_lossy(),
            &generated.generator,
            text,
            diagnostics,
        )?;
    }
    super::merge::apply_replacements(arena, acc, &applied.replacements, diagnostics)?;
    inputs.report_sites(
        &|site| applied.replacements.contains_key(&site.key),
        &failed,
        &consumed_ranges,
        diagnostics,
    );
    tracing::info!(
        "generators: {} ran in {:?} (snapshots {:?})",
        ran.join(", "),
        started.elapsed(),
        snapshotted
    );
    Ok(())
}

fn run_jobs<'g>(
    config: &Arc<ToolchainConfig>,
    jobs: Vec<super::apply::Job<'g>>,
    diagnostics: &mut DiagnosticBag,
) -> (Vec<super::apply::GenRun<'g>>, Vec<String>) {
    #[cfg(feature = "native")]
    return super::execute::execute(config, jobs, diagnostics);
    #[cfg(not(feature = "native"))]
    {
        let _ = (config, diagnostics);
        unreachable!(
            "generators run only with the native toolchain; got {} jobs",
            jobs.len()
        )
    }
}

/// Manifests retain every output kind for the language server and analyzer-only builds.
fn replay_materialized<'g>(
    entry_root: &std::path::Path,
    jobs: Vec<super::apply::Job<'g>>,
) -> (Vec<super::apply::GenRun<'g>>, Vec<String>) {
    let mut failed = Vec::new();
    let mut runs = Vec::new();
    for job in jobs {
        let dir = entry_root.join(&job.registered.name);
        let Ok(text) = std::fs::read_to_string(dir.join("outputs.json")) else {
            failed.push(job.registered.name.clone());
            continue;
        };
        let Ok(result) = serde_json::from_str::<super::model::GenResult>(&text) else {
            failed.push(job.registered.name.clone());
            continue;
        };
        runs.push(super::apply::GenRun {
            registered: job.registered,
            spans: job.built.spans,
            sites: job.built.sites,
            result,
        });
    }
    (runs, failed)
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::*;
    use crate::driver::generate::model::{GenResult, Output, SNAPSHOT_VERSION, Snapshot};

    #[test]
    fn materialized_manifests_replay_consumption_artifacts_and_assets() {
        let root = tempfile::tempdir().unwrap();
        let generator = RegisteredGenerator {
            name: "toy".into(),
            id: "::toy".into(),
            module: String::new(),
            file: "gen.dream".into(),
            std_package: None,
            span: None,
            attribute_triggers: Vec::new(),
            call_triggers: Vec::new(),
            syntax_block: false,
            incremental: true,
            tools: Vec::new(),
            entry: None,
        };
        let spans = || {
            let mut spans = super::super::decls::IdSpans::default();
            spans.consumable.insert(
                "::selected".into(),
                (
                    "main.dream".into(),
                    dream_text::text_span::TextSpan {
                        start: 0,
                        end: 8,
                        line_no: 1,
                        col_no: 1,
                    },
                ),
            );
            spans
        };
        let result = GenResult {
            version: SNAPSHOT_VERSION,
            generator: generator.name.clone(),
            outputs: vec![
                Output::Consume {
                    target: "::selected".into(),
                },
                Output::Artifact {
                    path: "resource.txt".into(),
                    contents: "ABC".into(),
                    mappings: Vec::new(),
                    publish: false,
                },
                Output::Asset {
                    name: "payload".into(),
                    source: "resource.txt".into(),
                    targets: Vec::new(),
                },
            ],
            ..Default::default()
        };
        let fresh = vec![super::super::apply::GenRun {
            registered: &generator,
            spans: spans(),
            sites: Default::default(),
            result,
        }];
        let mut diagnostics = DiagnosticBag::new(None);
        let applied = super::super::apply::apply(&fresh, root.path(), &mut diagnostics);
        assert!(!diagnostics.has_errors());
        super::super::materialize::materialize(
            root.path(),
            &fresh,
            &applied.files,
            &applied.artifacts,
        );
        let jobs = vec![super::super::apply::Job {
            registered: &generator,
            json: String::new(),
            built: super::super::snapshot::BuiltSnapshot {
                root: root.path().to_path_buf(),
                snapshot: Snapshot::default(),
                spans: spans(),
                sites: Default::default(),
            },
        }];
        let (replayed, failed) = replay_materialized(root.path(), jobs);
        assert!(failed.is_empty());
        let replayed = super::super::apply::apply(&replayed, root.path(), &mut diagnostics);
        assert!(!diagnostics.has_errors());
        assert_eq!(
            applied.consumed.keys().collect::<Vec<_>>(),
            replayed.consumed.keys().collect::<Vec<_>>()
        );
        assert_eq!(applied.assets, replayed.assets);
        assert_eq!(
            std::fs::read(root.path().join("toy/resource.txt")).unwrap(),
            b"ABC"
        );
    }
}
