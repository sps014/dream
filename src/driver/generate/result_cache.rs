//! Program-level generator result cache. The key is the compiler, the target, the generator
//! executables and the syntactic trigger set. A hit reuses the generated files when every
//! recorded module interface and consumed declaration still hashes the same, and skips semantic
//! analysis, snapshot expansion and lowering.

use super::apply::{Applied, GenRun};
use super::call_sites::FoundCall;
use super::decls::module_of;
use super::identity::compiler_identity;
use super::pass::{GenerateRequest, PassInputs};
use super::registry::RegisteredGenerator;
use super::sites::SiteKey;
use crate::driver::source_loader::ProgramAccumulator;
use bumpalo::Bump;
use dream_abi::attributes::{UserAttributes, decl_identity};
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{
    EnumDeclarationNode, FunctionNode, InterfaceDeclarationNode, StructDeclarationNode, Type,
};
use dream_syntax::token::syntax_token::SyntaxToken;
use indexmap::IndexMap;
use std::collections::BTreeMap;
use std::io::Error;
use std::path::{Path, PathBuf};

pub struct Hit {
    generators: Vec<String>,
    files: Vec<StoredFile>,
    replacements: IndexMap<SiteKey, String>,
    artifacts: Vec<StoredArtifact>,
    published: Vec<StoredBytes>,
    assets: Vec<StoredAsset>,
    consumed: Vec<StoredConsumed>,
    consumed_artifacts: Vec<(String, String)>,
    access: Vec<(String, Vec<String>)>,
    untracked_inputs: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Entry {
    generators: Vec<String>,
    files: Vec<StoredFile>,
    replacements: Vec<StoredReplacement>,
    artifacts: Vec<StoredArtifact>,
    published: Vec<StoredBytes>,
    assets: Vec<StoredAsset>,
    consumed: Vec<StoredConsumed>,
    consumed_artifacts: Vec<(String, String)>,
    /// Private declarations each generator's output may name; a hit skips the snapshot that
    /// normally grants this access.
    access: Vec<(String, Vec<String>)>,
    untracked_inputs: bool,
    modules: Vec<(String, String)>,
    decls: Vec<(String, String)>,
    /// Names and kinds of every declaration. A new type changes `type_index()` even when it is
    /// private, so it is not covered by module interface hashes.
    index: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredFile {
    generator: String,
    path: String,
    text: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredReplacement {
    start: usize,
    body: String,
    source: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct StoredArtifact {
    generator: String,
    path: String,
    contents: Vec<u8>,
    publish: bool,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct StoredBytes {
    path: String,
    bytes: Vec<u8>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct StoredAsset {
    name: String,
    bytes: Vec<u8>,
    targets: Vec<String>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct StoredConsumed {
    id: String,
    generator: String,
    help: String,
}

pub fn key(
    req: &GenerateRequest<'_>,
    acc: &ProgramAccumulator<'_>,
    attributes: &UserAttributes,
    inputs: &PassInputs,
) -> Option<String> {
    fingerprint(req, acc, attributes, inputs)
}

pub fn lookup(
    req: &GenerateRequest<'_>,
    acc: &ProgramAccumulator<'_>,
    inputs: &PassInputs,
    key: &str,
) -> Option<Hit> {
    if !enabled() {
        return None;
    }
    let path = cache_path(req, key);
    let entry: Entry = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
    if !dependencies_match(acc, &entry) || !covers_sites(inputs, &entry) {
        return None;
    }
    crate::driver::cache_budget::touch(&path);
    let mut replacements = IndexMap::new();
    for site in &inputs.sites {
        if let Some(replacement) = entry
            .replacements
            .iter()
            .find(|replacement| replacement.start == site.key.1 && replacement.body == site.body)
        {
            replacements.insert(site.key.clone(), replacement.source.clone());
        }
    }
    Some(Hit {
        generators: entry.generators,
        files: entry.files,
        replacements,
        artifacts: entry.artifacts,
        published: entry.published,
        assets: entry.assets,
        consumed: entry.consumed,
        consumed_artifacts: entry.consumed_artifacts,
        access: entry.access,
        untracked_inputs: entry.untracked_inputs,
    })
}

pub fn restore<'a>(
    req: &GenerateRequest<'_>,
    arena: &'a Bump,
    acc: &mut ProgramAccumulator<'a>,
    inputs: &PassInputs,
    hit: Hit,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), Error> {
    for (generator, ids) in &hit.access {
        acc.generator_access
            .entry(generator.clone())
            .or_default()
            .extend(ids.iter().cloned());
    }
    acc.untracked_generator_inputs |= hit.untracked_inputs;
    for consumed in &hit.consumed {
        let Some((file, span)) = declaration_span(acc, &consumed.id) else {
            continue;
        };
        acc.consumed.insert(
            consumed.id.clone(),
            dream_sema::module_graph::ConsumedBy {
                generator: consumed.generator.clone(),
                file,
                span,
                help: consumed.help.clone(),
            },
        );
    }
    for (id, path) in &hit.consumed_artifacts {
        let artifacts = acc.consumed_artifacts.entry(id.clone()).or_default();
        let path = PathBuf::from(path);
        if !artifacts.contains(&path) {
            artifacts.push(path);
        }
    }
    for output in &hit.published {
        acc.published_artifacts
            .insert(output.path.clone(), output.bytes.clone());
    }
    for artifact in &hit.artifacts {
        if artifact.publish {
            acc.published_artifacts
                .insert(artifact.path.clone(), artifact.contents.clone());
        }
        if let Ok(text) = std::str::from_utf8(&artifact.contents) {
            acc.file_contents.insert(
                super::paths::generator_dir(&entry_root(req, inputs), &artifact.generator)
                    .join(&artifact.path)
                    .to_string_lossy()
                    .into_owned(),
                text.to_string(),
            );
        }
    }
    for asset in &hit.assets {
        acc.embedded_assets
            .insert(asset.name.clone(), asset.bytes.clone());
    }
    let root = entry_root(req, inputs);
    let mut written: Vec<PathBuf> = Vec::new();
    for file in &hit.files {
        let path = super::paths::generator_dir(&root, &file.generator).join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
        }
        let bytes = file.text.as_bytes();
        if std::fs::read(&path).ok().as_deref() != Some(bytes) {
            std::fs::write(&path, bytes).map_err(|e| io_error(&path, e))?;
        }
        written.push(path.clone());
        acc.resolution_inputs.insert(path.clone());
        if inputs
            .gens
            .iter()
            .any(|generator| generator.id == file.generator && generator.is_std())
        {
            acc.std_generator_output_files
                .insert(path.to_string_lossy().into_owned());
        }
        super::merge::merge_generated_file(
            arena,
            acc,
            &path.to_string_lossy(),
            &file.generator,
            file.text.clone(),
            diagnostics,
        )?;
        tracing::info!("gen {}: result hit", file.generator);
    }
    remove_stale_outputs(&root, &hit, &written)?;
    if hit.files.is_empty() {
        for generator in &hit.generators {
            tracing::info!("gen {generator}: result hit");
        }
    }
    super::merge::apply_replacements(arena, acc, &hit.replacements, diagnostics)?;
    inputs.report_sites(
        &|site| hit.replacements.contains_key(&site.key),
        &[],
        &[],
        diagnostics,
    );
    tracing::info!("gen program: cache hit");
    Ok(())
}

pub fn store(
    req: &GenerateRequest<'_>,
    acc: &ProgramAccumulator<'_>,
    inputs: &PassInputs,
    runs: &[GenRun<'_>],
    applied: &Applied,
    key: &str,
) {
    if !enabled() {
        return;
    }
    let root = entry_root(req, inputs);
    let located = super::materialize::located(&root, &applied.files);
    let files = located
        .into_iter()
        .zip(&applied.files)
        .map(|((_, text), file)| StoredFile {
            generator: file.generator.clone(),
            path: file.path.clone(),
            text,
        })
        .collect();
    let entry = Entry {
        generators: runs.iter().map(|run| run.registered.id.clone()).collect(),
        files,
        replacements: applied
            .replacements
            .iter()
            .map(|((file, start), source)| StoredReplacement {
                start: *start,
                body: inputs
                    .sites
                    .iter()
                    .find(|site| site.key.0 == *file && site.key.1 == *start)
                    .map(|site| site.body.clone())
                    .unwrap_or_default(),
                source: source.clone(),
            })
            .collect(),
        artifacts: applied
            .artifacts
            .iter()
            .map(|artifact| StoredArtifact {
                generator: artifact.generator.clone(),
                path: artifact.path.clone(),
                contents: artifact.contents.clone(),
                publish: artifact.publish,
            })
            .collect(),
        published: applied
            .published
            .iter()
            .map(|output| StoredBytes {
                path: output.path.clone(),
                bytes: output.bytes.clone(),
            })
            .collect(),
        assets: applied
            .assets
            .iter()
            .map(|(name, (bytes, targets))| StoredAsset {
                name: name.clone(),
                bytes: bytes.clone(),
                targets: targets.clone(),
            })
            .collect(),
        consumed: applied
            .consumed
            .iter()
            .map(|(id, by)| StoredConsumed {
                id: id.clone(),
                generator: by.generator.clone(),
                help: by.help.clone(),
            })
            .collect(),
        consumed_artifacts: consumed_artifact_paths(acc, runs, &applied.consumed, &root),
        access: runs
            .iter()
            .filter_map(|run| {
                let ids = acc.generator_access.get(&run.registered.id)?;
                Some((run.registered.id.clone(), ids.iter().cloned().collect()))
            })
            .collect(),
        untracked_inputs: runs.iter().any(|run| !run.registered.incremental),
        modules: module_interfaces(acc).into_iter().collect(),
        decls: declaration_dependencies(acc, runs).into_iter().collect(),
        index: declaration_index(acc),
    };
    let Ok(bytes) = serde_json::to_vec(&entry) else {
        return;
    };
    let path = cache_path(req, key);
    let Some(dir) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let tmp = dir.join(format!(".{key}.{}.tmp", std::process::id()));
    if std::fs::write(&tmp, &bytes).is_err() {
        return;
    }
    if std::fs::rename(&tmp, &path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return;
    }
    crate::driver::cache_budget::touch(&path);
    crate::driver::cache_budget::enforce(dir, crate::driver::cache_budget::GENERATOR_RESULTS, &[]);
}

fn enabled() -> bool {
    std::env::var("DREAM_GENERATOR_CACHE").ok().as_deref() != Some("0")
}

fn cache_path(req: &GenerateRequest<'_>, key: &str) -> PathBuf {
    req.config
        .generator_cache_root()
        .join("program")
        .join(format!("{key}.json"))
}

fn entry_root(req: &GenerateRequest<'_>, inputs: &PassInputs) -> PathBuf {
    super::materialize::entry_dir(&inputs.paths.generated_dir(), req.entry_file)
}

fn remove_stale_outputs(root: &Path, hit: &Hit, written: &[PathBuf]) -> Result<(), Error> {
    for generator in &hit.generators {
        let dir = super::paths::generator_dir(root, generator);
        if !dir.is_dir() {
            continue;
        }
        let mut pending = vec![dir.clone()];
        while let Some(current) = pending.pop() {
            for entry in std::fs::read_dir(&current).map_err(|e| io_error(&current, e))? {
                let entry = entry.map_err(|e| io_error(&current, e))?;
                let path = entry.path();
                if entry.file_type().map_err(|e| io_error(&path, e))?.is_dir() {
                    pending.push(path);
                    continue;
                }
                let kept = path.file_name().is_some_and(|name| name == "outputs.json")
                    || written.iter().any(|item| item == &path)
                    || hit.artifacts.iter().any(|artifact| {
                        super::paths::generator_dir(root, &artifact.generator).join(&artifact.path)
                            == path
                    });
                if !kept {
                    std::fs::remove_file(&path).map_err(|e| io_error(&path, e))?;
                }
            }
        }
    }
    Ok(())
}

fn io_error(path: &Path, error: std::io::Error) -> Error {
    Error::new(error.kind(), format!("{}: {error}", path.display()))
}

fn fingerprint(
    req: &GenerateRequest<'_>,
    acc: &ProgramAccumulator<'_>,
    attributes: &UserAttributes,
    inputs: &PassInputs,
) -> Option<String> {
    let generators = applicable(inputs, acc, attributes);
    if generators.is_empty() {
        return None;
    }
    let identity = compiler_identity(req.config);
    let groups = super::exe::group(&generators);
    let plans: Vec<_> = groups
        .iter()
        .map(|group| super::exe::plan(req.config, &identity, &inputs.paths, group))
        .collect();
    let mut hash = blake3::Hasher::new();
    let mut put = |text: &str| {
        hash.update(&(text.len() as u64).to_le_bytes());
        hash.update(text.as_bytes());
    };
    put(&identity);
    put(&req.target.triple.to_string());
    put(&req.target.ptr_size.to_string());
    put(&req.target.ptr_align.to_string());
    for plan in &plans {
        put(&plan.key);
        for id in &plan.generators {
            put(id);
        }
    }
    for generator in &generators {
        for tool in super::tools::describe(req.config, &inputs.paths.root, &generator.tools) {
            put(&tool.name);
            put(&tool.version);
            put(&tool.fingerprint);
        }
    }
    let mut triggers = Vec::new();
    for call in &inputs.calls {
        let trigger = &inputs.call_triggers[call.trigger].id;
        let used = generators.iter().any(|generator| {
            generator
                .call_triggers
                .iter()
                .any(|candidate| &candidate.id == trigger)
        });
        if !used {
            continue;
        }
        let (text, relative) = enclosing(acc, call);
        triggers.push(format!(
            "call {trigger} {} {text} {relative}",
            call.file.as_deref().unwrap_or_default()
        ));
    }
    for (id, text) in attribute_declarations(acc, attributes, &generators) {
        triggers.push(format!("attr {id} {text}"));
    }
    for site in &inputs.sites {
        let Some(generator) = site.generator.as_deref() else {
            continue;
        };
        if !generators
            .iter()
            .any(|registered| registered.id == generator)
        {
            continue;
        }
        triggers.push(format!(
            "site {generator} {} {} {}",
            site.key.0,
            site.key.1,
            digest(&site.body)
        ));
    }
    triggers.sort();
    for trigger in &triggers {
        put(trigger);
    }
    Some(hash.finalize().to_hex().to_string())
}

fn applicable<'a>(
    inputs: &'a PassInputs,
    acc: &ProgramAccumulator<'_>,
    attributes: &UserAttributes,
) -> Vec<&'a RegisteredGenerator> {
    inputs
        .gens
        .iter()
        .filter(|generator| inputs.applicable(generator, acc, attributes))
        .collect()
}

fn covers_sites(inputs: &PassInputs, entry: &Entry) -> bool {
    inputs
        .sites
        .iter()
        .filter(|site| site.generator.is_some())
        .all(|site| {
            entry
                .replacements
                .iter()
                .any(|replacement| replacement.start == site.key.1 && replacement.body == site.body)
        })
}

fn dependencies_match(acc: &ProgramAccumulator<'_>, entry: &Entry) -> bool {
    if module_interfaces(acc) != entry.modules.iter().cloned().collect()
        || declaration_index(acc) != entry.index
    {
        return false;
    }
    entry
        .decls
        .iter()
        .all(|(id, hash)| declaration_hash(acc, id).is_some_and(|current| &current == hash))
}

fn module_interfaces(acc: &ProgramAccumulator<'_>) -> BTreeMap<String, String> {
    acc.module_graph()
        .module_hashes()
        .into_iter()
        .map(|module| (module.path, hex(&module.interface)))
        .collect()
}

fn declaration_index(acc: &ProgramAccumulator<'_>) -> String {
    let index = super::decls::DeclIndex::build(acc);
    let mut lines = Vec::new();
    for entry in index.types() {
        lines.push(format!("{} {}", entry.kind.as_str(), entry.id));
    }
    for entry in index.functions.values() {
        lines.push(format!("function {}", entry.id));
    }
    lines.sort();
    digest(&lines.join("\n"))
}

fn declaration_dependencies(
    acc: &ProgramAccumulator<'_>,
    runs: &[GenRun<'_>],
) -> BTreeMap<String, String> {
    let mut decls = BTreeMap::new();
    for run in runs {
        for id in run.spans.ids() {
            let Some(base) = id.split('#').next() else {
                continue;
            };
            if base.starts_with("site:") || base.starts_with("call:") || base.starts_with("module:")
            {
                continue;
            }
            if let Some(hash) = declaration_hash(acc, base) {
                decls.insert(base.to_string(), hash);
            }
        }
    }
    decls
}

fn consumed_artifact_paths(
    _acc: &ProgramAccumulator<'_>,
    runs: &[GenRun<'_>],
    consumed: &IndexMap<String, dream_sema::module_graph::ConsumedBy>,
    root: &Path,
) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    for run in runs {
        for output in &run.result.outputs {
            let super::model::Output::Artifact { path, mappings, .. } = output else {
                continue;
            };
            for (id, by) in consumed {
                let inside = |target: &str| {
                    target == id
                        || target
                            .strip_prefix(id.as_str())
                            .is_some_and(|rest| rest.starts_with(['#', '.']))
                };
                if by.generator != run.registered.id
                    || !mappings.iter().any(|mapping| inside(&mapping.target))
                {
                    continue;
                }
                let artifact = super::paths::generator_dir(root, &run.registered.id).join(path);
                paths.push((id.clone(), artifact.to_string_lossy().into_owned()));
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn attribute_declarations(
    acc: &ProgramAccumulator<'_>,
    attributes: &UserAttributes,
    generators: &[&RegisteredGenerator],
) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for generator in generators {
        if generator.attribute_triggers.is_empty() {
            continue;
        }
        let ids = &generator.attribute_triggers;
        for declaration in &acc.all_structs {
            if carries_type(
                &declaration.attributes,
                &declaration.fields,
                &declaration.methods,
                ids,
                attributes,
            ) {
                push_decl(
                    acc,
                    declaration.file_path.as_deref(),
                    &declaration.name.text,
                    &mut found,
                );
            }
        }
        for declaration in &acc.all_enums {
            if enum_carries(declaration, ids, attributes) {
                push_decl(
                    acc,
                    declaration.file_path.as_deref(),
                    &declaration.name.text,
                    &mut found,
                );
            }
        }
        for declaration in &acc.all_interfaces {
            if interface_carries(declaration, ids, attributes) {
                push_decl(
                    acc,
                    declaration.file_path.as_deref(),
                    &declaration.name.text,
                    &mut found,
                );
            }
        }
        for function in &acc.all_functions {
            if function_carries(function, ids, attributes) {
                push_decl(
                    acc,
                    function.file_path.as_deref(),
                    &function.name.text,
                    &mut found,
                );
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

fn push_decl(
    acc: &ProgramAccumulator<'_>,
    file: Option<&str>,
    name: &str,
    out: &mut Vec<(String, String)>,
) {
    let id = decl_identity(&module_of(acc, file), name);
    if let Some(hash) = declaration_hash(acc, &id) {
        out.push((id, hash));
    }
}

fn carries(attrs: &[dream_syntax::nodes::AttributeNode], ids: &[String]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.resolved_id.get().is_some_and(|id| ids.contains(id)))
}

fn function_carries(
    function: &FunctionNode<'_>,
    ids: &[String],
    _registry: &UserAttributes,
) -> bool {
    carries(&function.attributes, ids)
        || function
            .parameters
            .iter()
            .any(|parameter| carries(&parameter.attributes, ids))
}

fn carries_type(
    attributes: &[dream_syntax::nodes::AttributeNode],
    fields: &[dream_syntax::nodes::StructFieldNode],
    methods: &[FunctionNode<'_>],
    ids: &[String],
    registry: &UserAttributes,
) -> bool {
    carries(attributes, ids)
        || fields.iter().any(|field| carries(&field.attributes, ids))
        || methods
            .iter()
            .any(|method| function_carries(method, ids, registry))
}

fn enum_carries(
    declaration: &EnumDeclarationNode<'_>,
    ids: &[String],
    registry: &UserAttributes,
) -> bool {
    carries(&declaration.attributes, ids)
        || declaration.variants.iter().any(|variant| {
            variant
                .fields
                .iter()
                .any(|field| carries(&field.attributes, ids))
        })
        || declaration
            .methods
            .iter()
            .any(|method| function_carries(method, ids, registry))
}

fn interface_carries(
    declaration: &InterfaceDeclarationNode<'_>,
    ids: &[String],
    registry: &UserAttributes,
) -> bool {
    carries(&declaration.attributes, ids)
        || declaration
            .methods
            .iter()
            .any(|method| function_carries(method, ids, registry))
}

struct Cover {
    start: usize,
    end: usize,
}

impl Cover {
    fn new(token: &SyntaxToken) -> Self {
        Self {
            start: token.position.start,
            end: token.position.end,
        }
    }

    fn token(&mut self, token: &SyntaxToken) {
        self.start = self.start.min(token.position.start);
        self.end = self.end.max(token.position.end);
    }

    fn ty(&mut self, ty: &Type) {
        if let Some(span) = ty.get_span() {
            self.start = self.start.min(span.start);
            self.end = self.end.max(span.end);
        }
    }

    fn merge(&mut self, other: &Cover) {
        self.start = self.start.min(other.start);
        self.end = self.end.max(other.end);
    }
}

fn cover_function(function: &FunctionNode<'_>) -> Cover {
    let mut cover = Cover::new(&function.name);
    for attribute in &function.attributes {
        cover.token(&attribute.name);
    }
    for parameter in &function.parameters {
        cover.token(&parameter.name);
        cover.ty(&parameter.type_);
        for attribute in &parameter.attributes {
            cover.token(&attribute.name);
        }
    }
    if let Some(ret) = &function.return_type {
        cover.ty(ret);
    }
    for statement in function.body {
        if let Some(span) = statement.position() {
            cover.start = cover.start.min(span.start);
            cover.end = cover.end.max(span.end);
        }
    }
    cover
}

fn cover_struct(declaration: &StructDeclarationNode<'_>) -> Cover {
    let mut cover = Cover::new(&declaration.name);
    for attribute in &declaration.attributes {
        cover.token(&attribute.name);
    }
    for field in &declaration.fields {
        cover.token(&field.name);
        cover.token(&field.type_token);
        for attribute in &field.attributes {
            cover.token(&attribute.name);
        }
    }
    for method in &declaration.methods {
        cover.merge(&cover_function(method));
    }
    cover
}

fn declaration_hash(acc: &ProgramAccumulator<'_>, id: &str) -> Option<String> {
    let text = declaration_text(acc, id)?;
    Some(digest(&text))
}

fn declaration_span(
    acc: &ProgramAccumulator<'_>,
    id: &str,
) -> Option<(String, dream_text::text_span::TextSpan)> {
    let id = id.split('#').next()?;
    let (owner, member) = split_member(id);
    for declaration in &acc.all_structs {
        if identity(
            acc,
            declaration.file_path.as_deref(),
            &declaration.name.text,
        ) != owner
        {
            continue;
        }
        let token = match member {
            Some(member) => declaration
                .methods
                .iter()
                .find(|method| method.name.text == member)
                .map(|method| &method.name)?,
            None => &declaration.name,
        };
        return Some((
            declaration
                .file_path
                .as_deref()
                .unwrap_or_default()
                .to_string(),
            token.position,
        ));
    }
    for function in &acc.all_functions {
        if member.is_none()
            && identity(acc, function.file_path.as_deref(), &function.name.text) == owner
        {
            return Some((
                function
                    .file_path
                    .as_deref()
                    .unwrap_or_default()
                    .to_string(),
                function.name.position,
            ));
        }
    }
    None
}

fn declaration_text(acc: &ProgramAccumulator<'_>, id: &str) -> Option<String> {
    let id = id.split('#').next()?;
    let (owner, member) = split_member(id);
    let mut parts = Vec::new();
    for declaration in &acc.all_structs {
        if identity(
            acc,
            declaration.file_path.as_deref(),
            &declaration.name.text,
        ) != owner
        {
            continue;
        }
        match member {
            Some(member) => {
                for method in declaration
                    .methods
                    .iter()
                    .filter(|method| method.name.text == member)
                {
                    parts.push(slice(
                        acc,
                        declaration.file_path.as_deref(),
                        &cover_function(method),
                    )?);
                }
            }
            None => parts.push(slice(
                acc,
                declaration.file_path.as_deref(),
                &cover_struct(declaration),
            )?),
        }
    }
    for declaration in &acc.all_enums {
        if identity(
            acc,
            declaration.file_path.as_deref(),
            &declaration.name.text,
        ) != owner
        {
            continue;
        }
        let mut cover = Cover::new(&declaration.name);
        for attribute in &declaration.attributes {
            cover.token(&attribute.name);
        }
        for variant in &declaration.variants {
            cover.token(&variant.name);
            for field in &variant.fields {
                cover.token(&field.name);
                cover.token(&field.type_token);
            }
        }
        for method in &declaration.methods {
            if member.is_some_and(|name| method.name.text != name) {
                continue;
            }
            cover.merge(&cover_function(method));
        }
        if member.is_none() || cover.start != declaration.name.position.start {
            parts.push(slice(acc, declaration.file_path.as_deref(), &cover)?);
        }
    }
    for declaration in &acc.all_interfaces {
        if identity(
            acc,
            declaration.file_path.as_deref(),
            &declaration.name.text,
        ) != owner
        {
            continue;
        }
        let mut cover = Cover::new(&declaration.name);
        for method in &declaration.methods {
            if member.is_some_and(|name| method.name.text != name) {
                continue;
            }
            cover.merge(&cover_function(method));
        }
        parts.push(slice(acc, declaration.file_path.as_deref(), &cover)?);
    }
    if member.is_none() {
        for function in &acc.all_functions {
            if identity(acc, function.file_path.as_deref(), &function.name.text) != owner {
                continue;
            }
            parts.push(slice(
                acc,
                function.file_path.as_deref(),
                &cover_function(function),
            )?);
        }
    }
    for extension in &acc.all_extends {
        let owner_id = identity(acc, extension.file_path.as_deref(), &extension.target.text);
        for method in &extension.methods {
            let method_id = format!("{owner_id}.{}", method.name.text);
            if method_id == id || (member == Some(method.name.text.as_str()) && owner_id == owner) {
                parts.push(slice(
                    acc,
                    extension.file_path.as_deref(),
                    &cover_function(method),
                )?);
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.concat())
    }
}

fn identity(acc: &ProgramAccumulator<'_>, file: Option<&str>, name: &str) -> String {
    decl_identity(&module_of(acc, file), name)
}

fn split_member(id: &str) -> (&str, Option<&str>) {
    match id.rsplit_once('.') {
        Some((owner, member)) if !member.contains("::") => (owner, Some(member)),
        _ => (id, None),
    }
}

fn enclosing(acc: &ProgramAccumulator<'_>, call: &FoundCall) -> (String, usize) {
    let pos = call.token.position.start;
    let file = call.file.as_deref();
    let mut best: Option<(usize, usize, String)> = None;
    let mut consider = |function: &FunctionNode<'_>, owner_file: Option<&str>| {
        let function_file = function.file_path.as_deref().or(owner_file);
        if file.is_some() && function_file != file {
            return;
        }
        let cover = cover_function(function);
        if pos < cover.start || pos >= cover.end {
            return;
        }
        let Some(text) = slice(acc, function_file, &cover) else {
            return;
        };
        let len = cover.end - cover.start;
        if best.as_ref().is_none_or(|(_, best_len, _)| len < *best_len) {
            best = Some((cover.start, len, digest(&text)));
        }
    };
    for function in &acc.all_functions {
        consider(function, None);
    }
    for declaration in &acc.all_structs {
        for method in &declaration.methods {
            consider(method, declaration.file_path.as_deref());
        }
    }
    for declaration in &acc.all_enums {
        for method in &declaration.methods {
            consider(method, declaration.file_path.as_deref());
        }
    }
    for declaration in &acc.all_interfaces {
        for method in &declaration.methods {
            consider(method, declaration.file_path.as_deref());
        }
    }
    for extension in &acc.all_extends {
        for method in &extension.methods {
            consider(method, extension.file_path.as_deref());
        }
    }
    match best {
        Some((start, _, hash)) => (hash, pos - start),
        None => (
            digest(
                file.and_then(|path| acc.file_contents.get(path))
                    .map(String::as_str)
                    .unwrap_or_default(),
            ),
            pos,
        ),
    }
}

fn slice(acc: &ProgramAccumulator<'_>, file: Option<&str>, cover: &Cover) -> Option<String> {
    let source = acc.file_contents.get(file?)?;
    let end = cover.end.min(source.len());
    let start = cover.start.min(end);
    Some(source[start..end].to_string())
}

fn digest(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
