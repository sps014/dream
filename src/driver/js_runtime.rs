//! Selective JS runtime emission: assemble a tree-shaken `<stem>.web.runtime.js` /
//! `<stem>.node.runtime.js` next to each `.wasm` from the modular sources under `runtime/src/`,
//! including only the host chunks required by live WASM imports. Opt-in via CLI
//! `--runtime --web` / `--runtime --node` (both hosts may be requested in one compile).
//!
//! The loader itself is the canonical [`runtime/src/load.js`] (same file the full
//! `runtime/dream.js` bundle uses); this module only picks which chunk files join it and
//! generates the data-driven `defaultDreamModule` composer. The chunk table lives in
//! `runtime/src/chunks.manifest`, shared with `scripts/bundle-runtime.mjs`.

use std::collections::BTreeSet;
use std::fs;
use std::io::Error;
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::driver::abi::LiveImport;

/// Target host for a selective runtime (CLI `--web` / `--node`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JsRuntimeTarget {
    /// Browser: `isNode = false`, no `node:*` preloads; `fetch` for bytes.
    Web,
    /// Node ≥ 18: `isNode = true`, preload `node:fs` / `node:crypto` when those chunks are present.
    Node,
}

impl JsRuntimeTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            JsRuntimeTarget::Web => "web",
            JsRuntimeTarget::Node => "node",
        }
    }

    /// Sibling filename extension for this host (`web.runtime.js` / `node.runtime.js`).
    pub fn runtime_extension(self) -> String {
        format!("{}.runtime.js", self.as_str())
    }
}

/// One optional host chunk from `chunks.manifest`.
struct Chunk {
    id: String,
    file: String,
    /// Host-factory function defined by `file`; `None` when wired elsewhere (`workers`).
    factory: Option<String>,
    /// Import-field matchers: `(exact, name)` — prefix match unless `exact`.
    fields: Vec<(bool, String)>,
}

/// Parsed `runtime/src/chunks.manifest`.
struct Manifest {
    always: Vec<String>,
    order: Vec<String>,
    chunks: Vec<Chunk>,
}

fn parse_manifest(text: &'static str) -> Manifest {
    let mut manifest = Manifest {
        always: Vec::new(),
        order: Vec::new(),
        chunks: Vec::new(),
    };
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut tokens = line.split_whitespace();
        match tokens.next() {
            Some("always") => manifest.always.extend(tokens.map(String::from)),
            Some("order") => manifest.order.extend(tokens.map(String::from)),
            Some("chunk") => {
                let (Some(id), Some(file), Some(factory)) =
                    (tokens.next(), tokens.next(), tokens.next())
                else {
                    panic!("malformed chunks.manifest line: {}", line);
                };
                manifest.chunks.push(Chunk {
                    id: id.to_string(),
                    file: file.to_string(),
                    factory: (factory != "-").then(|| factory.to_string()),
                    fields: tokens
                        .map(|f| {
                            (
                                f.starts_with('='),
                                f.strip_prefix('=').unwrap_or(f).to_string(),
                            )
                        })
                        .collect(),
                });
            }
            other => panic!("unknown chunks.manifest directive {:?}", other),
        }
    }
    manifest
}
fn runtime_src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime/src")
}

/// Strip ESM imports / rewrite exports so modules can share one scope when concatenated.
fn transform_module(text: &str, rel: &str) -> String {
    let mut cleaned = String::new();
    // Multi-line `import { … } from "…";` statements span several lines; skip until the
    // terminating `;` once one starts.
    let mut in_import = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if in_import {
            if trimmed.contains(';') {
                in_import = false;
            }
            continue;
        }
        if trimmed.starts_with("import ") {
            if !trimmed.ends_with(';') {
                in_import = true;
            }
            continue;
        }
        if trimmed.starts_with("export {") || trimmed.starts_with("export default") {
            continue;
        }
        let mut line = line.to_string();
        for (from, to) in [
            ("export async function ", "async function "),
            ("export function ", "function "),
            ("export class ", "class "),
            ("export const ", "const "),
            ("export let ", "let "),
        ] {
            if let Some(rest) = line.trim_start().strip_prefix(from) {
                let indent_len = line.len() - line.trim_start().len();
                line = format!("{}{}{}", &line[..indent_len], to, rest);
                break;
            }
        }
        cleaned.push_str(&line);
        cleaned.push('\n');
    }

    format!("\n// ----- {} -----\n{}", rel, cleaned.trim())
}

/// Pin `isNode` for the selected host so host chunks take the right branch without probing.
fn pin_is_node(text: &str, target: JsRuntimeTarget) -> String {
    let pinned = match target {
        JsRuntimeTarget::Web => "const isNode = false;",
        JsRuntimeTarget::Node => "const isNode = true;",
    };
    // platform.js becomes `const isNode = <runtime detect>;` after transform.
    if let Some(idx) = text.find("const isNode =") {
        let after = &text[idx..];
        if let Some(end) = after.find(';') {
            let mut out = String::with_capacity(text.len());
            out.push_str(&text[..idx]);
            out.push_str(pinned);
            out.push_str(&after[end + 1..]);
            return out;
        }
    }
    format!("{}\n{}\n", pinned, text)
}

/// Assemble a selective runtime from live import `(module, field)` pairs for `target`.
pub(crate) fn assemble_selective_runtime(
    live_imports: &[LiveImport],
    target: JsRuntimeTarget,
) -> Result<String, Error> {
    let manifest = parse_manifest(include_str!("../../runtime/src/chunks.manifest"));
    let mut chunks: BTreeSet<&str> = BTreeSet::new();
    for (_module, field) in live_imports {
        if let Some(c) = chunk_for_field(&manifest, field) {
            chunks.insert(c);
        }
    }
    // GPU resources are `js` handles; the module emits `js_retain`/`js_release` even when no
    // other `js*` bridge survives import pruning.
    if chunks.contains("gpu") {
        chunks.insert("js");
    }

    let src = runtime_src_dir();
    let mut out = format!(
        "// Dream JS interop runtime (selective — generated per compile for {}).\n\
         // Only host chunks required by this module's live imports are included.\n\
         // Full runtime: runtime/dream.js (from runtime/src via scripts/bundle-runtime.mjs).\n\
         // Emit with: dream --runtime --{} <file.dream>\n",
        target.as_str(),
        target.as_str(),
    );

    for rel in &manifest.always {
        let path = src.join(rel);
        let text = fs::read_to_string(&path)?;
        let mut transformed = transform_module(&text, rel);
        if rel == "platform.js" {
            transformed = pin_is_node(&transformed, target);
        }
        out.push_str(&transformed);
        out.push('\n');
    }

    for chunk_id in &manifest.order {
        if !chunks.contains(chunk_id.as_str()) {
            continue;
        }
        let Some(chunk) = manifest.chunks.iter().find(|c| &c.id == chunk_id) else {
            continue;
        };
        let path = src.join(&chunk.file);
        let text = fs::read_to_string(&path)?;
        out.push_str(&transform_module(&text, &chunk.file));
        out.push('\n');
    }

    // Data-driven host composer over exactly the included chunks (the full-bundle
    // `hosts.js` references every factory, which would be undefined here).
    out.push_str("\n// ----- selective Dream host composer -----\n");
    out.push_str("function defaultDreamModule(getInstance) {\n  const parts = {};\n");
    for chunk in &manifest.chunks {
        if !chunks.contains(chunk.id.as_str()) {
            continue;
        }
        if let Some(factory) = &chunk.factory {
            out.push_str(&format!("  Object.assign(parts, {factory}(getInstance));\n"));
        }
    }
    out.push_str("  return parts;\n}\n");

    let loader = fs::read_to_string(src.join("load.js"))?;
    out.push_str(&transform_module(&loader, "load.js"));
    out.push_str(
        "\nexport { load, run, DreamInstance, TAGS, HEAP_HEADER_SIZE };\n\
         export default { load, run, DreamInstance, TAGS, HEAP_HEADER_SIZE };\n",
    );
    Ok(out)
}

/// Map a Dream host import field to the optional chunk that implements it.
fn chunk_for_field<'a>(manifest: &'a Manifest, field: &str) -> Option<&'a str> {
    for id in &manifest.order {
        let Some(chunk) = manifest.chunks.iter().find(|c| &c.id == id) else {
            continue;
        };
        for &(exact, ref name) in &chunk.fields {
            if (exact && field == name) || (!exact && field.starts_with(name.as_str())) {
                return Some(&chunk.id);
            }
        }
    }
    None
}

/// Writes `<wat_stem>.{web,node}.runtime.js` next to the compiled `.wat` / `.wasm` for each
/// target. Optimizing builds (`-O` / `--release`) minify the emitted JS; a minifier failure
/// falls back to the readable form rather than failing the compile. Returns the paths written.
pub(crate) fn emit_selective_runtimes(
    wat_path: &str,
    live_imports: &[LiveImport],
    targets: &[JsRuntimeTarget],
    minify: bool,
) -> Result<Vec<std::path::PathBuf>, Error> {
    let mut written = Vec::new();
    for &target in targets {
        let path = Path::new(wat_path).with_extension(target.runtime_extension());
        let text = assemble_selective_runtime(live_imports, target)?;
        let final_text = if minify {
            match minify_js_source(&text) {
                Ok(m) => m,
                Err(e) => {
                    debug!("could not minify {}: {}", path.display(), e);
                    text
                }
            }
        } else {
            text
        };
        fs::write(&path, final_text)?;
        written.push(path.clone());
        if minify {
            // Same release gate as minification: emit .gz/.br sidecars for static servers.
            for (sidecar, _) in crate::driver::compress::write_precompressed(&path) {
                written.push(sidecar);
            }
        }
    }
    Ok(written)
}

/// Minifies an ES module with the Oxc toolchain (parse → compress/mangle → codegen).
fn minify_js_source(source: &str) -> Result<String, String> {
    let allocator = oxc_allocator::Allocator::default();
    let source_type = oxc_span::SourceType::mjs();
    let parser_return = oxc_parser::Parser::new(&allocator, source, source_type).parse();
    if !parser_return.diagnostics.is_empty() {
        return Err(
            parser_return
                .diagnostics
                .first()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "parse failed".to_string()),
        );
    }
    let mut program = parser_return.program;
    oxc_minifier::Minifier::new(oxc_minifier::MinifierOptions::default())
        .minify(&allocator, &mut program);
    Ok(oxc_codegen::Codegen::new()
        .with_options(oxc_codegen::CodegenOptions::default())
        .build(&program)
        .code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_print_runtime_omits_gpu_and_fs() {
        let text = assemble_selective_runtime(&[], JsRuntimeTarget::Web).expect("assemble");
        assert!(!text.contains("makeGpuHost(getInstance)"));
        assert!(!text.contains("makeFsHost(getInstance)"));
        assert!(!text.contains("makeCryptoHost(getInstance)"));
        assert!(text.contains("function load("));
        assert!(text.contains("readEmbeddedAbi"));
        assert!(text.contains("const isNode = false;"));
    }

    #[test]
    fn node_target_pins_is_node() {
        let text = assemble_selective_runtime(&[], JsRuntimeTarget::Node).expect("assemble");
        assert!(text.contains("const isNode = true;"));
    }

    #[test]
    fn gpu_field_pulls_gpu_chunk() {
        let live = vec![("Dream".into(), "gpuDispatch".into())];
        let text = assemble_selective_runtime(&live, JsRuntimeTarget::Web).expect("assemble");
        assert!(text.contains("makeGpuHost(getInstance)"));
        assert!(text.contains("makeJsHost(getInstance)"));
        assert!(!text.contains("makeFsHost(getInstance)"));
    }

    #[test]
    fn compiler_emitted_js_rc_is_bound_from_wasm_imports() {
        let text = assemble_selective_runtime(&[], JsRuntimeTarget::Web).expect("assemble");
        assert!(text.contains(r#"imp.name === "jsRetain" || imp.name === "jsRelease""#));
    }

    #[test]
    fn selective_runtime_is_deterministic() {
        let live = vec![
            ("Dream".into(), "jsGlobal".into()),
            ("Dream".into(), "fileRead".into()),
        ];
        let a = assemble_selective_runtime(&live, JsRuntimeTarget::Node).unwrap();
        let b = assemble_selective_runtime(&live, JsRuntimeTarget::Node).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn manifest_fields_match_previous_prefix_table() {
        let manifest = parse_manifest(include_str!("../../runtime/src/chunks.manifest"));
        assert_eq!(chunk_for_field(&manifest, "jsGlobal"), Some("js"));
        assert_eq!(chunk_for_field(&manifest, "fileRead"), Some("fs"));
        assert_eq!(chunk_for_field(&manifest, "dirList"), Some("fs"));
        assert_eq!(chunk_for_field(&manifest, "delayMs"), Some("datetime_text"));
        assert_eq!(chunk_for_field(&manifest, "delay"), None);
        assert_eq!(chunk_for_field(&manifest, "__attachGpuAbi"), Some("gpu"));
        assert_eq!(chunk_for_field(&manifest, "print_int"), None);
    }
}
