use super::model::{GenResult, Output};
use indexmap::IndexSet;

/// A cache entry or manifest cannot introduce tool files that no recorded phase requested.
pub(super) fn validate(result: &GenResult, declared: &[String]) -> Result<(), String> {
    if result.phases.len() > 3 {
        return Err("generator transcript exceeds 3 phases".into());
    }
    if !result.phases.is_empty()
        && !result
            .phases
            .iter()
            .flat_map(|p| &p.outputs)
            .eq(&result.outputs)
    {
        return Err("generator outputs do not match the phase transcript".into());
    }
    for (i, phase) in result.phases.iter().enumerate() {
        if phase.snapshot.version != super::model::SNAPSHOT_VERSION
            || phase.snapshot.phase != i as u32
            || phase.snapshot.generator != result.generator
            || phase
                .snapshot
                .tools
                .iter()
                .any(|t| !declared.contains(&t.name))
        {
            return Err("invalid generator phase snapshot".into());
        }
        if let Some(previous) = i.checked_sub(1).map(|p| &result.phases[p]) {
            let ids: Vec<_> = previous
                .outputs
                .iter()
                .filter_map(|o| match o {
                    Output::ToolRun { id, .. } => Some(id),
                    _ => None,
                })
                .collect();
            if !ids
                .iter()
                .copied()
                .eq(phase.snapshot.tool_results.iter().map(|r| &r.id))
            {
                return Err(
                    "generator feedback does not match the preceding phase requests".into(),
                );
            }
            let mut base = previous.snapshot.clone();
            base.phase = phase.snapshot.phase;
            base.tool_results = phase.snapshot.tool_results.clone();
            if base != phase.snapshot {
                return Err("generator compilation changed between feedback phases".into());
            }
        }
    }
    let mut ids = IndexSet::new();
    for output in &result.outputs {
        if let Output::ToolRun {
            id,
            tool,
            inputs,
            outputs,
            ..
        } = output
            && (result.phases.is_empty()
                || !ids.insert(id)
                || !declared.contains(tool)
                || !inputs
                    .iter()
                    .chain(outputs)
                    .all(|p| super::artifacts::valid_relative_path(p) && p != "outputs.json"))
        {
            return Err("invalid or undeclared tool request in generator transcript".into());
        }
    }
    let mut results = IndexSet::new();
    for feedback in &result.tool_results {
        let request = result
            .outputs
            .iter()
            .find_map(|o| match o {
                Output::ToolRun { id, outputs, .. } if id == &feedback.id => Some(outputs),
                _ => None,
            })
            .ok_or_else(|| "tool feedback has no request".to_string())?;
        if !results.insert(&feedback.id) {
            return Err("duplicate tool feedback".into());
        }
        let mut files = IndexSet::new();
        for file in &feedback.files {
            if !request.contains(&file.path) || !files.insert(&file.path) {
                return Err("unexpected tool output file".into());
            }
            super::assets::file_bytes(file)?;
        }
    }
    for phase in &result.phases {
        for feedback in &phase.snapshot.tool_results {
            if !result.tool_results.contains(feedback) {
                return Err("phase contains unrecorded tool feedback".into());
            }
        }
    }
    if !result.diagnostics.iter().any(|d| d.severity != "warning") && ids.len() != results.len() {
        return Err("generator transcript has missing tool feedback".into());
    }
    Ok(())
}
