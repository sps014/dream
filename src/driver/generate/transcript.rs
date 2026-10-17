use super::model::{GenResult, Output};
use indexmap::IndexSet;
use std::collections::BTreeSet;

/// A cache entry or manifest cannot introduce tool files that no recorded phase requested. Fresh
/// and replayed results go through the same checks: each phase's feedback must answer exactly the
/// preceding phase's requests, and every recorded file must resolve to a blob with its hash.
pub(super) fn validate(result: &GenResult, declared: &[String]) -> Result<(), String> {
    if result.phases.len() > 3 {
        return Err("generator transcript exceeds 3 phases".into());
    }
    if !result.phases.is_empty() {
        if result.base_snapshot.len() != 64
            || !result.base_snapshot.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err("generator transcript has no base snapshot hash".into());
        }
        if !result
            .phases
            .iter()
            .flat_map(|p| &p.outputs)
            .eq(&result.outputs)
        {
            return Err("generator outputs do not match the phase transcript".into());
        }
    }
    let mut ids = IndexSet::new();
    for output in &result.outputs {
        if let Output::ToolRun { id, .. } = output
            && (result.phases.is_empty()
                || !ids.insert(id)
                || !super::artifacts::valid_tool_request(output, declared))
        {
            return Err("invalid or undeclared tool request in generator transcript".into());
        }
    }
    let mut referenced = BTreeSet::new();
    for (i, phase) in result.phases.iter().enumerate() {
        let requests: Vec<_> = i
            .checked_sub(1)
            .map(|p| &result.phases[p].outputs)
            .into_iter()
            .flatten()
            .filter_map(|o| match o {
                Output::ToolRun {
                    id,
                    outputs,
                    opaque_outputs,
                    ..
                } => Some((id, outputs, opaque_outputs)),
                _ => None,
            })
            .collect();
        if !requests
            .iter()
            .map(|(id, ..)| *id)
            .eq(phase.tool_results.iter().map(|r| &r.id))
        {
            return Err("generator feedback does not match the preceding phase requests".into());
        }
        for (feedback, (_, outputs, opaque_outputs)) in phase.tool_results.iter().zip(&requests) {
            let mut files = IndexSet::new();
            for file in &feedback.files {
                let readable = outputs.contains(&file.path);
                if !(readable || opaque_outputs.contains(&file.path))
                    || file.opaque == readable
                    || !file.contents.is_empty()
                    || !files.insert(&file.path)
                {
                    return Err("unexpected tool output file".into());
                }
                let bytes = super::assets::blob_bytes(&result.blobs, &file.hash)?;
                if bytes.len() as u64 != file.size {
                    return Err(format!("tool output '{}' has an invalid size", file.path));
                }
                referenced.insert(&file.hash);
            }
        }
    }
    if result.blobs.keys().any(|hash| !referenced.contains(hash)) {
        return Err("generator transcript stores an unreferenced tool output".into());
    }
    let answered = result
        .phases
        .iter()
        .map(|p| p.tool_results.len())
        .sum::<usize>();
    if !result.diagnostics.iter().any(|d| d.severity != "warning") && ids.len() != answered {
        return Err("generator transcript has missing tool feedback".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::model::{GenPhase, ToolFile, ToolResult};
    use super::*;

    fn transcript(contents: &[u8]) -> GenResult {
        let hash = blake3::hash(contents).to_hex().to_string();
        let mut blobs = std::collections::BTreeMap::new();
        super::super::assets::store_blob(&mut blobs, &hash, contents);
        let request = Output::ToolRun {
            id: "compile".into(),
            tool: "copy".into(),
            args: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            opaque_outputs: vec!["out.bin".into()],
        };
        GenResult {
            base_snapshot: "0".repeat(64),
            outputs: vec![request.clone()],
            phases: vec![
                GenPhase {
                    tool_results: Vec::new(),
                    outputs: vec![request],
                },
                GenPhase {
                    tool_results: vec![ToolResult {
                        id: "compile".into(),
                        files: vec![ToolFile {
                            path: "out.bin".into(),
                            hash,
                            size: contents.len() as u64,
                            opaque: true,
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    outputs: Vec::new(),
                },
            ],
            blobs,
            ..Default::default()
        }
    }

    #[test]
    fn opaque_feedback_resolves_through_content_addressed_blobs() {
        let declared = vec!["copy".to_string()];
        assert_eq!(validate(&transcript(b"\0spirv"), &declared), Ok(()));
        let mut forged = transcript(b"\0spirv");
        forged.blobs.values_mut().for_each(|v| *v = "AAAA".into());
        assert!(validate(&forged, &declared).unwrap_err().contains("hash"));
        let mut readable = transcript(b"\0spirv");
        readable.phases[1].tool_results[0].files[0].opaque = false;
        assert!(validate(&readable, &declared).is_err());
        let mut unanswered = transcript(b"\0spirv");
        unanswered.phases.pop();
        assert!(validate(&unanswered, &declared).is_err());
    }
}
