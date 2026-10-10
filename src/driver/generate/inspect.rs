//! What the generate pass would do for a program, without running or merging anything: the
//! registered generators, whether each fires, its snapshot, and its executable plan. Backs
//! `dream generate` and `dream debug-adapter --generator`.

use super::exe::ExePlan;
use super::pass::{GenerateRequest, gather};
use super::registry::RegisteredGenerator;
use super::snapshot::has_inputs;
use crate::driver::source_loader::ProgramAccumulator;
use dream_abi::attributes::UserAttributes;
use dream_diagnostics::DiagnosticBag;
use std::path::PathBuf;

pub struct InspectedGenerator {
    pub registered: RegisteredGenerator,
    pub applicable: bool,
    pub sites: usize,
    pub calls: usize,
    /// The canonical snapshot the generator would receive, when it fires and has inputs.
    pub snapshot_json: Option<String>,
    pub plan: ExePlan,
    /// `@incremental` result-cache key for `snapshot_json`.
    pub result_key: Option<String>,
}

pub struct GenInspection {
    pub entry: String,
    pub generated_root: PathBuf,
    pub generators: Vec<InspectedGenerator>,
}

impl GenInspection {
    pub fn find(&self, name: &str) -> Option<&InspectedGenerator> {
        self.generators.iter().find(|g| g.registered.name == name)
    }
}

pub fn inspect(
    req: &GenerateRequest<'_>,
    acc: &ProgramAccumulator<'_>,
    attributes: &UserAttributes,
    diagnostics: &mut DiagnosticBag,
) -> GenInspection {
    let inputs = gather(req, acc, attributes, diagnostics);
    let identity = super::identity::compiler_identity(req.config);
    let refs: Vec<&RegisteredGenerator> = inputs.gens.iter().collect();
    let groups = super::exe::group(&refs);
    let plans: Vec<ExePlan> = groups
        .iter()
        .map(|g| super::exe::plan(req.config, &identity, g))
        .collect();
    let mut generators = Vec::with_capacity(inputs.gens.len());
    for registered in &inputs.gens {
        let Some(plan) = plans
            .iter()
            .find(|p| p.generators.contains(&registered.name))
        else {
            continue;
        };
        let applicable = inputs.applicable(registered, acc, attributes);
        let snapshot_json = if applicable {
            inputs
                .snapshot(acc, attributes, registered)
                .ok()
                .filter(|b| has_inputs(&b.snapshot))
                .and_then(|b| serde_json::to_string(&b.snapshot).ok())
        } else {
            None
        };
        let result_key = snapshot_json
            .as_deref()
            .filter(|_| registered.incremental)
            .map(|json| super::incremental::result_key(&plan.key, json));
        generators.push(InspectedGenerator {
            sites: inputs
                .sites
                .iter()
                .filter(|s| s.name == registered.name)
                .count(),
            calls: inputs
                .calls
                .iter()
                .filter(|c| {
                    let id = &inputs.call_triggers[c.trigger].id;
                    registered.call_triggers.iter().any(|t| &t.id == id)
                })
                .count(),
            registered: registered.clone(),
            applicable,
            snapshot_json,
            plan: plan.clone(),
            result_key,
        });
    }
    GenInspection {
        entry: req.entry_file.to_string(),
        generated_root: inputs.generated_root(req.config, req.entry_file),
        generators,
    }
}
