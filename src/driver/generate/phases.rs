use super::exe::GenExe;
use super::model::{GenPhase, GenResult, Output};
use crate::driver::toolchain::ToolchainConfig;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Every phase sees the original compilation plus the compiler-collected tool feedback.
pub(super) fn run(
    config: &Arc<ToolchainConfig>,
    exe: &GenExe,
    generator: &str,
    declared: &[String],
    json: &str,
    timeout: u64,
    root: &std::path::Path,
) -> Result<GenResult, String> {
    let mut snapshot: super::model::Snapshot =
        serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut aggregate = GenResult {
        version: snapshot.version,
        generator: snapshot.generator.clone(),
        ..Default::default()
    };
    let mut artifacts = BTreeMap::new();
    let mut request_ids = indexmap::IndexSet::new();
    for phase in 0..3 {
        snapshot.phase = phase;
        let json = serde_json::to_string(&snapshot).map_err(|e| e.to_string())?;
        let result = super::run::run(config, exe, generator, &json, timeout)?;
        aggregate.phases.push(GenPhase {
            snapshot: snapshot.clone(),
            outputs: result.outputs.clone(),
        });
        aggregate.logs.extend(result.logs);
        let errors = result.diagnostics.iter().any(|d| d.severity != "warning");
        aggregate.diagnostics.extend(result.diagnostics);
        for output in &result.outputs {
            if let Output::Artifact { path, contents, .. } = output
                && (!super::artifacts::valid_artifact_path(path)
                    || artifacts
                        .insert(path.clone(), contents.as_bytes().to_vec())
                        .is_some())
            {
                return Err(format!("invalid or duplicate tool input artifact '{path}'"));
            }
        }
        let requests: Vec<_> = result
            .outputs
            .iter()
            .filter(|o| matches!(o, Output::ToolRun { .. }))
            .collect();
        aggregate.outputs.extend(result.outputs.iter().cloned());
        if requests.is_empty() || errors {
            return Ok(aggregate);
        }
        if phase == 2 {
            return Err(
                "generator requested a tool in the final feedback round (maximum 3 phases)".into(),
            );
        }
        snapshot.tool_results.clear();
        for request in requests {
            if !super::tools::valid_request(request, declared) {
                return Err("invalid request or undeclared generator tool".into());
            }
            let Output::ToolRun { id, .. } = request else {
                continue;
            };
            if !request_ids.insert(id.clone()) {
                return Err(format!("duplicate tool request id '{id}'"));
            }
            let result = super::tools::run(config, root, request, &snapshot.tools, &artifacts)?;
            for file in &result.files {
                let bytes = super::assets::file_bytes(file)?;
                if artifacts.insert(file.path.clone(), bytes).is_some() {
                    return Err(format!("duplicate tool output '{}'", file.path));
                }
            }
            aggregate.tool_results.push(result.clone());
            snapshot.tool_results.push(result);
        }
    }
    Ok(aggregate)
}
