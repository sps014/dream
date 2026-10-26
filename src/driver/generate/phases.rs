use super::exe::GenExe;
use super::model::{GenPhase, GenResult, Output, ToolResult};
use crate::driver::toolchain::ToolchainConfig;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Every phase sees the original compilation plus the compiler-collected tool feedback. The
/// transcript records the base snapshot's hash and each phase's feedback, never a phase snapshot.
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
        super::model::Snapshot::decode(json).map_err(|e| e.to_string())?;
    let mut aggregate = GenResult {
        version: snapshot.version,
        generator: snapshot.generator_id.clone(),
        base_snapshot: blake3::hash(json.as_bytes()).to_hex().to_string(),
        ..Default::default()
    };
    let mut artifacts = BTreeMap::new();
    let mut request_ids = indexmap::IndexSet::new();
    let mut feedback: Vec<ToolResult> = Vec::new();
    for phase in 0..3 {
        let result = if phase == 0 {
            super::run::run(config, exe, generator, json, timeout)?
        } else {
            snapshot.phase = phase;
            let json = serde_json::to_string(&snapshot).map_err(|e| e.to_string())?;
            super::run::run(config, exe, generator, &json, timeout)?
        };
        aggregate.phases.push(GenPhase {
            tool_results: std::mem::take(&mut feedback),
            outputs: result.outputs.clone(),
        });
        aggregate.logs.extend(result.logs);
        let errors = result.diagnostics.iter().any(|d| d.severity != "warning");
        aggregate.diagnostics.extend(result.diagnostics);
        for output in &result.outputs {
            if let Output::Artifact {
                path,
                contents,
                binary,
                ..
            } = output
                && (!super::artifacts::valid_artifact_path(path)
                    || super::artifacts::artifact_bytes(contents, *binary)
                        .is_none_or(|bytes| artifacts.insert(path.clone(), bytes).is_some()))
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
        for request in &requests {
            if !super::artifacts::valid_tool_request(request, declared) {
                return Err("invalid request or undeclared generator tool".into());
            }
            if let Output::ToolRun { id, .. } = request
                && !request_ids.insert(id.clone())
            {
                return Err(format!("duplicate tool request id '{id}'"));
            }
        }
        let runs = super::parallel::map(&requests, |request| {
            super::tools::run(config, root, request, &snapshot.tools, &artifacts)
        });
        snapshot.tool_results.clear();
        for run in runs {
            let run = run?;
            let mut shown = run.result.clone();
            for (file, bytes) in shown.files.iter_mut().zip(&run.bytes) {
                *file = super::assets::snapshot_file(file, bytes);
                if artifacts.insert(file.path.clone(), bytes.clone()).is_some() {
                    return Err(format!("duplicate tool output '{}'", file.path));
                }
                super::assets::store_blob(&mut aggregate.blobs, &file.hash, bytes);
            }
            snapshot.tool_results.push(shown);
            feedback.push(run.result);
        }
    }
    Ok(aggregate)
}
