//! Produces a result for every generator job: `@incremental` replays first, then the cached
//! executables the remaining jobs need are built in parallel, then those jobs run in parallel.
//! Results come back in job (generator name) order.

use super::apply::{GenRun, Job};
use super::exe::{self, ExePlan};
use super::incremental;
use super::model::GenResult;
use super::registry::RegisteredGenerator;
use super::stats::{record, Event};
use crate::driver::toolchain::ToolchainConfig;
use dream_diagnostics::{Diagnostic, DiagnosticBag};
use std::sync::Arc;

enum Outcome {
    Done(GenResult, bool),
    Failed(String),
}

pub fn execute<'g>(
    config: &Arc<ToolchainConfig>,
    jobs: Vec<Job<'g>>,
    diagnostics: &mut DiagnosticBag,
) -> (Vec<GenRun<'g>>, Vec<String>) {
    let identity = super::identity::compiler_identity(config);
    let gens: Vec<&RegisteredGenerator> = jobs.iter().map(|j| j.registered).collect();
    let groups = exe::group(&gens);
    let plans: Vec<ExePlan> = super::parallel::map(&groups, |g| exe::plan(config, &identity, g));
    let plan_of = |registered: &RegisteredGenerator| {
        plans
            .iter()
            .position(|p| p.generators.contains(&registered.name))
            .unwrap_or_default()
    };
    let cache_root = config.generator_cache_root();

    let mut outcomes: Vec<Option<Outcome>> = jobs
        .iter()
        .map(|job| {
            if !job.registered.incremental {
                return None;
            }
            let key = incremental::result_key(&plans[plan_of(job.registered)].key, &job.json);
            incremental::load(&cache_root, &key).map(|r| {
                record(Event::Replay);
                tracing::info!("gen {}: result hit", job.registered.name);
                Outcome::Done(r, true)
            })
        })
        .collect();

    let needed: Vec<usize> = (0..plans.len())
        .filter(|&p| {
            jobs.iter()
                .zip(&outcomes)
                .any(|(job, o)| o.is_none() && plan_of(job.registered) == p)
        })
        .collect();
    let built = super::parallel::map(&needed, |&p| {
        let started = std::time::Instant::now();
        let result = exe::ensure_built(config, &plans[p]);
        let names = plans[p].generators.join(", ");
        match &result {
            Ok((_, true)) => {
                record(Event::ExeBuild);
                tracing::info!("gen {names}: exe miss (build {:?})", started.elapsed());
            }
            Ok((_, false)) => {
                record(Event::ExeCacheHit);
                tracing::info!("gen {names}: exe hit");
            }
            Err(_) => {}
        }
        (p, result.map(|(exe, _)| exe))
    });

    let pending: Vec<usize> = (0..jobs.len()).filter(|&i| outcomes[i].is_none()).collect();
    let fresh = super::parallel::map(&pending, |&i| {
        let job = &jobs[i];
        let p = plan_of(job.registered);
        let Some((_, exe)) = built.iter().find(|(bp, _)| *bp == p) else {
            return Outcome::Failed(format!("generator '{}': no executable", job.registered.name));
        };
        let exe = match exe {
            Ok(exe) => exe,
            Err(e) => return Outcome::Failed(format!("generator '{}': {e}", job.registered.name)),
        };
        let timeout = job
            .registered
            .entry
            .as_ref()
            .and_then(|e| e.timeout_secs)
            .unwrap_or(super::run::DEFAULT_TIMEOUT_SECS);
        record(Event::Run);
        let started = std::time::Instant::now();
        let outcome = match super::run::run(config, exe, &job.registered.name, &job.json, timeout) {
            Ok(result) => Outcome::Done(result, false),
            Err(e) => Outcome::Failed(e),
        };
        tracing::info!("gen {}: run {:?}", job.registered.name, started.elapsed());
        outcome
    });
    for (i, outcome) in pending.into_iter().zip(fresh) {
        outcomes[i] = Some(outcome);
    }

    let mut runs = Vec::with_capacity(jobs.len());
    let mut failed = Vec::new();
    for (job, outcome) in jobs.into_iter().zip(outcomes) {
        match outcome {
            Some(Outcome::Done(result, replayed)) => {
                let clean = result.diagnostics.iter().all(|d| d.severity == "warning");
                if job.registered.incremental && !replayed && clean {
                    let key = incremental::result_key(&plans[plan_of(job.registered)].key, &job.json);
                    incremental::store(&cache_root, &key, &result);
                }
                runs.push(GenRun {
                    registered: job.registered,
                    spans: job.built.spans,
                    sites: job.built.sites,
                    result,
                });
            }
            Some(Outcome::Failed(message)) => {
                failed.push(job.registered.name.clone());
                diagnostics.report(Diagnostic::new(
                    message,
                    job.registered.span,
                    Some(job.registered.file.clone()),
                ));
            }
            None => {}
        }
    }
    (runs, failed)
}
