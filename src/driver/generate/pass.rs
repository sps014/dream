//! The generate pass: discover generators, skip everything when no trigger matches, otherwise
//! snapshot → replay or run → validate → materialize → merge.

use super::call_sites::{FoundCall, collect_calls};
use super::decls::DeclIndex;
use super::paths::ProjectPaths;
use super::registry::{CallTrigger, RegisteredGenerator, Registration};
use super::sites::{Site, collect_sites, report_unexpanded};
use super::snapshot::{BuiltSnapshot, ProgramFacts, build_snapshot, has_inputs, program_carries};
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
    PassInputs {
        sites: collect_sites(acc),
        calls: collect_calls(acc, &call_triggers),
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
        diagnostics: &mut DiagnosticBag,
    ) {
        let claimed = |name: &str| self.gens.iter().any(|g| g.syntax_block && g.name == name);
        report_unexpanded(&self.sites, &claimed, diagnostics);
        for site in &self.sites {
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
        inputs.report_sites(&|_| false, &[], diagnostics);
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
    let applied = super::apply::apply(&runs, diagnostics);
    failed.extend(applied.failed.iter().cloned());
    let ran: Vec<String> = runs.iter().map(|r| r.registered.name.clone()).collect();
    let files = if req.replay_materialized {
        super::materialize::located(&root, &applied.files)
    } else {
        super::materialize::materialize(&root, &ran, &applied.files)
    };
    for (path, text) in files {
        acc.resolution_inputs.insert(path.clone());
        super::merge::merge_generated_file(arena, acc, &path.to_string_lossy(), text, diagnostics)?;
    }
    super::merge::apply_replacements(arena, acc, &applied.replacements, diagnostics)?;
    inputs.report_sites(
        &|site| applied.replacements.contains_key(&site.key),
        &failed,
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

/// Results rebuilt from the files the last build materialized (the only option without the
/// native toolchain). Syntax sites stay unexpanded,
/// so every job also counts as failed (its sites are not reported again).
fn replay_materialized<'g>(
    entry_root: &std::path::Path,
    jobs: Vec<super::apply::Job<'g>>,
) -> (Vec<super::apply::GenRun<'g>>, Vec<String>) {
    let mut failed = Vec::new();
    let mut runs = Vec::new();
    for job in jobs {
        failed.push(job.registered.name.clone());
        let dir = entry_root.join(&job.registered.name);
        let header = super::materialize::header(&job.registered.name);
        let mut files: Vec<std::path::PathBuf> = crate::driver::rt_stamp::files_under(&dir);
        files.sort();
        let outputs = files
            .into_iter()
            .filter_map(|p| {
                let text = std::fs::read_to_string(&p).ok()?;
                let rel = p
                    .strip_prefix(&dir)
                    .ok()?
                    .to_string_lossy()
                    .replace('\\', "/");
                Some(super::model::Output::File {
                    path: rel,
                    source: text.strip_prefix(&header).unwrap_or(&text).to_string(),
                })
            })
            .collect();
        runs.push(super::apply::GenRun {
            registered: job.registered,
            spans: job.built.spans,
            sites: job.built.sites,
            result: super::model::GenResult {
                outputs,
                ..Default::default()
            },
        });
    }
    (runs, failed)
}
