use super::apply::GenRun;
#[cfg(feature = "native")]
use super::apply::Job;
use super::model::Output;
#[cfg(feature = "native")]
use super::model::{GenPhase, GenResult, SNAPSHOT_VERSION};
#[cfg(feature = "native")]
use super::pass::{GenerateRequest, PassInputs};
#[cfg(feature = "native")]
use crate::driver::source_loader::ProgramAccumulator;
#[cfg(feature = "native")]
use dream_abi::attributes::UserAttributes;
#[cfg(feature = "native")]
use dream_diagnostics::{Diagnostic, DiagnosticBag};
use dream_text::text_span::TextSpan;

/// Identifies consumed body ranges without granting rights beyond the input snapshot.
pub(super) fn ranges(runs: &[GenRun<'_>]) -> Vec<(String, String, TextSpan)> {
    runs.iter()
        .flat_map(|run| {
            run.result.outputs.iter().filter_map(|output| {
                let Output::Consume { target, .. } = output else {
                    return None;
                };
                run.spans.consumable.get(target)?;
                let (file, span) = run.spans.get(&format!("{target}#b.0"))?;
                Some((run.registered.name.clone(), file?.to_string(), span))
            })
        })
        .collect()
}

/// Consumers are discovered before other generators' final site snapshots can be fixed.
#[cfg(feature = "native")]
pub(super) fn stabilize<'g>(
    req: &GenerateRequest<'_>,
    inputs: &'g PassInputs,
    acc: &ProgramAccumulator<'_>,
    attributes: &UserAttributes,
    mut runs: Vec<GenRun<'g>>,
    failed: &mut Vec<String>,
    diagnostics: &mut DiagnosticBag,
) -> Vec<GenRun<'g>> {
    for round in 0..8 {
        let consumed = ranges(&runs);
        if round == 0 && consumed.is_empty() {
            return runs;
        }
        let mut jobs = Vec::new();
        for run in &runs {
            let registered = run.registered;
            if run.result.phases.is_empty() {
                continue;
            }
            let Ok(mut built) = inputs.snapshot(acc, attributes, registered) else {
                continue;
            };
            let excluded = |id: &str| {
                built.spans.get(id).is_some_and(|(file, point)| {
                    consumed.iter().any(|(owner, source, span)| {
                        owner != &registered.name
                            && file == Some(source.as_str())
                            && span.start <= point.start
                            && point.start < span.end
                    })
                })
            };
            built.snapshot.blocks.retain(|s| !excluded(&s.id));
            built.snapshot.calls.retain(|s| !excluded(&s.id));
            let Ok(json) = serde_json::to_string(&built.snapshot) else {
                continue;
            };
            if blake3::hash(json.as_bytes()).to_hex().as_str() == run.result.base_snapshot {
                continue;
            }
            built
                .sites
                .retain(|id, _| built.snapshot.blocks.iter().any(|b| &b.id == id));
            jobs.push(Job {
                registered,
                built,
                json,
            });
        }
        if jobs.is_empty() {
            return runs;
        }
        let mut pending = Vec::new();
        let mut skipped = Vec::new();
        for job in jobs {
            if super::snapshot::has_inputs(&job.built.snapshot) {
                pending.push(job);
            } else {
                skipped.push(GenRun {
                    registered: job.registered,
                    spans: job.built.spans,
                    sites: job.built.sites,
                    result: GenResult {
                        version: SNAPSHOT_VERSION,
                        generator: job.registered.name.clone(),
                        base_snapshot: blake3::hash(job.json.as_bytes()).to_hex().to_string(),
                        phases: vec![GenPhase {
                            tool_results: Vec::new(),
                            outputs: Vec::new(),
                        }],
                        ..Default::default()
                    },
                });
            }
        }
        let (mut replacements, errors) = super::execute::execute(req.config, pending, diagnostics);
        failed.extend(errors.iter().cloned());
        replacements.extend(skipped);
        for replacement in replacements {
            if let Some(run) = runs
                .iter_mut()
                .find(|r| r.registered.name == replacement.registered.name)
            {
                *run = replacement;
            }
        }
        runs.retain(|r| !errors.contains(&r.registered.name));
    }
    diagnostics.report(Diagnostic::new(
        "generator consumption did not stabilize after excluding consumed-body sites".to_string(),
        None,
        Some(req.entry_file.to_string()),
    ));
    runs
}
