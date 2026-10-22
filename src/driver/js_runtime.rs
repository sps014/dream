//! Selective JS runtime emission: assemble a tree-shaken `<stem>.web.runtime.js` /
//! `<stem>.node.runtime.js` next to each `.wasm` from the modular sources under `runtime/src/`,
//! including only the host chunks required by live WASM imports. Opt-in via CLI
//! `--runtime --web` / `--runtime --node` (both hosts may be requested in one compile).
//!
//! The loader itself is the canonical [`runtime/src/load.js`] (same file the full
//! `runtime/dream.js` bundle uses); this module only picks which chunk files join it and
//! generates the data-driven `defaultDreamModule` composer. The chunk table lives in
//! `runtime/src/chunks.manifest`, shared with `scripts/bundle-runtime.mjs`.

mod companions;
mod host_exports;
mod requirements;
mod typed;

use std::collections::BTreeSet;
use std::fs;
use std::io::Error;
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::driver::abi::LiveImport;
use requirements::Requirements;

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
    files: Vec<String>,
    /// Host-factory function defined by the chunk; `None` when wired elsewhere (`workers`).
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
                    files: file.split(',').map(String::from).collect(),
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
pub(crate) fn runtime_src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime/src")
}

/// Strip ESM imports / rewrite exports so modules can share one scope when concatenated.
fn transform_module(text: &str, rel: &str) -> Result<String, Error> {
    use oxc_ast::ast::Statement;
    use oxc_span::GetSpan;

    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, text, oxc_span::SourceType::mjs()).parse();
    if let Some(diagnostic) = parsed.diagnostics.first() {
        return Err(Error::other(format!("{rel}: {diagnostic}")));
    }
    let mut cleaned = String::new();
    let mut cursor = 0;
    // Module declarations alone are rewritten; worker source inside template strings stays intact.
    for statement in &parsed.program.body {
        let (start, end) = match statement {
            Statement::ImportDeclaration(import) => (import.span.start, import.span.end),
            Statement::ExportDeclaration(export) => {
                (export.span.start, export.declaration.span().start)
            }
            Statement::ExportNamedDeclaration(export) => (export.span.start, export.span.end),
            Statement::ExportFromDeclaration(export) => (export.span.start, export.span.end),
            Statement::ExportDefaultDeclaration(export) => (export.span.start, export.span.end),
            Statement::ExportAllDeclaration(export) => (export.span.start, export.span.end),
            _ => continue,
        };
        cleaned.push_str(&text[cursor..start as usize]);
        cursor = end as usize;
    }
    cleaned.push_str(&text[cursor..]);
    Ok(format!("\n// ----- {} -----\n{}", rel, cleaned.trim()))
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
#[cfg(test)]
fn assemble_selective_runtime(
    live_imports: &[LiveImport],
    target: JsRuntimeTarget,
) -> Result<String, Error> {
    assemble_runtime(live_imports, target, true, "dream-data.wasm")
}

#[cfg(test)]
fn assemble_runtime(
    live_imports: &[LiveImport],
    target: JsRuntimeTarget,
    async_imports: bool,
    codecs_file: &str,
) -> Result<String, Error> {
    assemble_for_requirements(
        live_imports,
        target,
        Requirements::full(async_imports),
        codecs_file,
    )
}

fn assemble_for_requirements(
    live_imports: &[LiveImport],
    target: JsRuntimeTarget,
    requirements: Requirements,
    codecs_file: &str,
) -> Result<String, Error> {
    let manifest = parse_manifest(include_str!("../../runtime/src/chunks.manifest"));
    let mut chunks: BTreeSet<&str> = BTreeSet::new();
    let fields: BTreeSet<_> = live_imports
        .iter()
        .filter_map(|(module, field)| (module == "Dream").then_some(field.as_str()))
        .collect();
    let env_fields: BTreeSet<_> = live_imports
        .iter()
        .filter_map(|(module, field)| (module == "env").then_some(field.as_str()))
        .collect();
    for field in &fields {
        if let Some(c) = chunk_for_field(&manifest, field) {
            chunks.insert(c);
        }
    }
    let async_imports = requirements.async_imports || chunks.contains("cancellation");
    if async_imports {
        chunks.insert("async");
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
        if rel == "features.js" {
            out.push_str(&requirements.constants());
            out.push_str(&format!(
                "const HAS_ASYNC_IMPORTS = {async_imports};\nconst HAS_DATA_CODECS = {};\nconst DATA_CODEC_FILES = {};\n",
                chunks.contains("data"),
                serde_json::to_string(&[("dataCompression", "compression")].into_iter()
                    .filter(|(field, _)| fields.contains(field))
                    .map(|(field, family)| (field, format!("{}.{family}.wasm", codecs_file.strip_suffix(".data.wasm").unwrap_or("dream"))))
                    .collect::<std::collections::BTreeMap<_,_>>()).map_err(Error::other)?,
            ));
            continue;
        }
        let path = src.join(rel);
        let text = fs::read_to_string(&path)?;
        let mut transformed = transform_module(&text, rel)?;
        if rel == "platform.js" {
            transformed = pin_is_node(&transformed, target);
        }
        if rel == "hosts/env.js" {
            transformed = host_exports::select(&transformed, "defaultEnv", &env_fields)?;
        }
        if rel == "instance.js" {
            transformed = requirements.select_instance(&transformed)?;
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
        let mut text = String::new();
        for file in &chunk.files {
            text.push_str(&transform_module(
                &fs::read_to_string(src.join(file))?,
                file,
            )?);
            text.push('\n');
        }
        if let Some(factory) = &chunk.factory {
            text = host_exports::select(&text, factory, &fields)?;
        }
        out.push_str(&text);
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
            out.push_str(&format!(
                "  Object.assign(parts, {factory}(getInstance));\n"
            ));
        }
    }
    out.push_str("  return parts;\n}\n");

    let loader = fs::read_to_string(src.join("load.js"))?;
    out.push_str(&transform_module(&loader, "load.js")?);
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
    wasm_bytes: &[u8],
    targets: &[JsRuntimeTarget],
    minify: bool,
    assets: &indexmap::IndexMap<String, Vec<u8>>,
) -> Result<Vec<std::path::PathBuf>, Error> {
    // WASI archives and runtime adapters can add host imports after MIR import pruning.
    let mut live_imports = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(wasm_bytes) {
        if let wasmparser::Payload::ImportSection(imports) = payload.map_err(Error::other)? {
            for import in imports.into_imports() {
                let import = import.map_err(Error::other)?;
                if matches!(import.ty, wasmparser::TypeRef::Func(_)) {
                    live_imports.push((import.module.to_string(), import.name.to_string()));
                }
            }
        }
    }
    let mut written = Vec::new();
    let abi_path = Path::new(wat_path).with_extension("abi.json");
    let abi: serde_json::Value =
        serde_json::from_slice(&fs::read(&abi_path)?).map_err(Error::other)?;
    let requirements = Requirements::from_abi(&abi, &live_imports)?;
    let codecs_path = Path::new(wat_path).with_extension("data.wasm");
    let codecs_file = codecs_path
        .file_name()
        .ok_or_else(|| Error::other("missing WASM artifact filename"))?
        .to_string_lossy();
    if !targets.is_empty() {
        written.extend(companions::emit(
            Path::new(wat_path),
            &live_imports,
            minify,
        )?);
    }
    for &target in targets {
        let path = Path::new(wat_path).with_extension(target.runtime_extension());
        let mut text =
            assemble_for_requirements(&live_imports, target, requirements, &codecs_file)?;
        text = text.replace(
            "function bindTypedImport() { return null; }",
            &typed::generate(&abi, &live_imports)?,
        );
        let entries: Vec<_> = assets.iter().collect();
        let entries = serde_json::to_string(&entries).map_err(Error::other)?;
        text.push_str(&format!("\nexport const embeddedAssets = Object.freeze(Object.fromEntries({entries}.map(([name, bytes]) => [name, Uint8Array.from(bytes)])));\n"));
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
        return Err(parser_return
            .diagnostics
            .first()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "parse failed".to_string()));
    }
    let mut program = parser_return.program;
    let minified = oxc_minifier::Minifier::new(oxc_minifier::MinifierOptions::default())
        .minify(&allocator, &mut program);
    Ok(oxc_codegen::Codegen::new()
        .with_options(oxc_codegen::CodegenOptions::minify())
        .with_scoping(minified.scoping)
        .build(&program)
        .code)
}

#[cfg(test)]
mod tests;
