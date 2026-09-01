//! The `tower_lsp` server: owns per-document state and translates protocol requests into queries
//! over the symbol [`Index`] and the diagnostics front-end.
//!
//! Three things make it "seamless": documents are synced **incrementally** (only the changed
//! range is applied), the built [`Index`] is **cached per document version** so repeated
//! navigation requests on an unchanged document are free, and `publishDiagnostics` is
//! **debounced** so a burst of keystrokes only triggers one analysis pass.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{jsonrpc, Client, LanguageServer};

use crate::analysis;
use crate::conversions::{completion_kind, map_position, map_range, symbol_kind};
use crate::index::{self, Index};
use crate::position::LineIndex;
use crate::semantic_tokens;

/// How long to wait after the last edit before publishing diagnostics. A newer edit arriving
/// within the window cancels the pending pass.
const DIAGNOSTIC_DEBOUNCE: Duration = Duration::from_millis(200);

/// The current contents and version of one open document.
#[derive(Debug, Clone)]
struct Document {
    text: String,
    version: i32,
}

/// A symbol index cached against the document version it was built from.
#[derive(Debug, Clone)]
struct CachedIndex {
    version: i32,
    index: Arc<Index>,
    /// The analyzer's IDE snapshot for the same version, when semantic analysis completed
    /// without panicking. Powers type-aware completion/hover; `None` falls back to the
    /// AST-index heuristics.
    sema: Option<Arc<dream_sema::analyzer::IdeSnapshot>>,
}

#[derive(Debug)]
pub struct Backend {
    client: Client,
    documents: Arc<DashMap<String, Document>>,
    index_cache: Arc<DashMap<String, CachedIndex>>,
    /// The most recently scheduled diagnostics version per document, used to debounce/cancel
    /// superseded passes.
    pending_diagnostics: Arc<DashMap<String, i32>>,
    /// Cached on-disk workspace symbol scan (see [`crate::workspace`]); invalidated by
    /// file-watch events and refreshed after a short TTL.
    workspace_cache: Arc<tokio::sync::Mutex<Option<crate::workspace::WorkspaceIndex>>>,
}

/// Writes the embedded stdlib source for a `<std>/…` virtual path into `cache_dir` (once, kept
/// current on toolchain changes) and returns the real path, so go-to-definition can open it in
/// the editor like any other file.
pub fn materialize_stdlib(cache_dir: &std::path::Path, virtual_path: &str) -> Option<String> {
    let rel = virtual_path.strip_prefix("<std>/")?;
    for pkg in dream_stdlib::STD_PACKAGES {
        for &(vpath, source) in pkg.files {
            if vpath != virtual_path {
                continue;
            }
            let target = cache_dir.join(rel);
            let stale = std::fs::read_to_string(&target)
                .map(|existing| existing != source)
                .unwrap_or(true);
            if stale {
                target
                    .parent()
                    .and_then(|p| std::fs::create_dir_all(p).ok())?;
                std::fs::write(&target, source).ok()?;
            }
            return target.to_str().map(str::to_string);
        }
    }
    None
}

/// True when an enclosing `dream.toml` declares `type = "lib"` (no Run/Debug CodeLens).
fn workspace_is_lib_package(file_path: &str) -> bool {
    let mut dir = std::path::Path::new(file_path)
        .parent()
        .map(|p| p.to_path_buf());
    while let Some(d) = dir {
        let manifest = d.join("dream.toml");
        if manifest.is_file() {
            if let Ok(text) = std::fs::read_to_string(&manifest) {
                // Match `[package]` then a `type = "lib"` / `type = 'lib'` line before the next table.
                let mut in_package = false;
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with('[') {
                        in_package = trimmed == "[package]";
                        continue;
                    }
                    if !in_package {
                        continue;
                    }
                    if let Some(rest) = trimmed.strip_prefix("type") {
                        let rest = rest.trim().trim_start_matches('=').trim();
                        let val = rest.trim_matches('"').trim_matches('\'');
                        return val == "lib";
                    }
                }
            }
            return false;
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
    false
}

impl Backend {
    pub fn new(client: Client) -> Backend {
        Backend {
            client,
            documents: Arc::new(DashMap::new()),
            index_cache: Arc::new(DashMap::new()),
            pending_diagnostics: Arc::new(DashMap::new()),
            workspace_cache: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    fn file_path_of(uri: &Url) -> Option<String> {
        uri.to_file_path()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
    }

    /// Builds an LSP [`Location`] for a byte span, optionally in another on-disk file. Virtual
    /// `<std>/…` paths are materialized to real files first (see [`materialize_stdlib`]).
    fn location_at(
        default_uri: &Url,
        default_text: &str,
        start: usize,
        end: usize,
        file_path: Option<&str>,
    ) -> Option<Location> {
        let file_path: Option<String> = match file_path {
            Some(path) if path.starts_with('<') => {
                Self::stdlib_cache_dir().and_then(|dir| materialize_stdlib(&dir, path))
            }
            other => other.map(str::to_string),
        };
        match file_path {
            None => {
                let line_index = LineIndex::new(default_text);
                Some(Location {
                    uri: default_uri.clone(),
                    range: Range {
                        start: map_position(line_index.position(start)),
                        end: map_position(line_index.position(end)),
                    },
                })
            }
            Some(path) => {
                let path_buf = std::path::Path::new(&path);
                if !path_buf.is_file() {
                    return None;
                }
                let text = std::fs::read_to_string(path_buf).ok()?;
                let uri = Url::from_file_path(path_buf).ok()?;
                let line_index = LineIndex::new(&text);
                Some(Location {
                    uri,
                    range: Range {
                        start: map_position(line_index.position(start)),
                        end: map_position(line_index.position(end)),
                    },
                })
            }
        }
    }

    /// The directory embedded-stdlib sources are materialized into for navigation. Overridable
    /// via `DREAM_LSP_STD_CACHE` (tests); defaults to the toolchain's own home.
    fn stdlib_cache_dir() -> Option<std::path::PathBuf> {
        if let Some(dir) = std::env::var_os("DREAM_LSP_STD_CACHE") {
            return Some(std::path::PathBuf::from(dir));
        }
        std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .map(|home| home.join(".dream").join("std"))
    }

    /// When the analyzer resolved `offset` to a cross-file-capable entity (anything but a
    /// function-local or an anonymous expression), returns that target plus the receiver-typed
    /// reference spans for it in this document (declaration included). Returns `None` for local
    /// symbols, where scope-based index matching is already exact.
    fn precise_references(
        idx: &Index,
        snapshot: &dream_sema::analyzer::IdeSnapshot,
        offset: usize,
    ) -> Option<(dream_sema::analyzer::ide::IdeTarget, Vec<(usize, usize)>)> {
        use dream_sema::analyzer::ide::IdeTarget;
        let r = snapshot.ref_covering(offset)?;
        if matches!(r.target, IdeTarget::Local { .. } | IdeTarget::Expr) {
            return None;
        }
        let mut spans = crate::sema_ide::references_in(snapshot, &r.target);
        if let Some((ds, de)) = crate::sema_ide::definition_at(snapshot, idx, offset) {
            spans.push((ds, de));
        }
        spans.sort_unstable();
        spans.dedup();
        Some((r.target.clone(), spans))
    }

    /// Returns the current text of a document, if open.
    fn document_text(&self, uri: &str) -> Option<String> {
        self.documents.get(uri).map(|d| d.text.clone())
    }

    /// Returns the symbol index (and analyzer snapshot) for a document, rebuilding both only
    /// when the cached version is stale (or absent). Results are shared via [`Arc`] so callers
    /// never clone the model.
    fn models_for(
        &self,
        uri: &str,
        file_path: Option<&str>,
    ) -> Option<(Arc<Index>, Option<Arc<dream_sema::analyzer::IdeSnapshot>>)> {
        let (text, version) = {
            let doc = self.documents.get(uri)?;
            if let Some(cached) = self.index_cache.get(uri) {
                if cached.version == doc.version {
                    return Some((cached.index.clone(), cached.sema.clone()));
                }
            }
            (doc.text.clone(), doc.version)
        };

        let index = Arc::new(Index::build(file_path, &text));
        let sema = analysis::analyze_document(file_path, &text)
            .sema
            .map(Arc::new);
        self.index_cache.insert(
            uri.to_string(),
            CachedIndex {
                version,
                index: index.clone(),
                sema: sema.clone(),
            },
        );
        Some((index, sema))
    }

    /// Returns the symbol index for a document, rebuilding it only when the cached version is
    /// stale (or absent).
    fn index_for(&self, uri: &str, file_path: Option<&str>) -> Option<Arc<Index>> {
        self.models_for(uri, file_path).map(|(index, _)| index)
    }

    /// Schedules a debounced diagnostics pass for `uri` at `version`. If a newer version is
    /// scheduled before the debounce elapses, this pass is dropped.
    fn schedule_diagnostics(&self, uri: Url, text: String, version: i32) {
        let key = uri.to_string();
        self.pending_diagnostics.insert(key.clone(), version);

        let client = self.client.clone();
        let pending = self.pending_diagnostics.clone();
        let file_path = Self::file_path_of(&uri);

        tokio::spawn(async move {
            tokio::time::sleep(DIAGNOSTIC_DEBOUNCE).await;
            // Bail out if a newer edit superseded this pass while we were waiting.
            if pending.get(&key).map(|v| *v) != Some(version) {
                return;
            }
            let diagnostics = compute_diagnostics(file_path.as_deref(), &text);
            client
                .publish_diagnostics(uri, diagnostics, Some(version))
                .await;
        });
    }
}

/// Runs the front-end and maps its output to protocol diagnostics.
fn compute_diagnostics(file_path: Option<&str>, text: &str) -> Vec<Diagnostic> {
    analysis::collect_diagnostics(file_path, text)
        .into_iter()
        .map(|d| Diagnostic {
            range: map_range(d.range),
            severity: match d.severity {
                "error" => Some(DiagnosticSeverity::ERROR),
                "warning" => Some(DiagnosticSeverity::WARNING),
                _ => Some(DiagnosticSeverity::INFORMATION),
            },
            message: d.message,
            code: d.code.map(|c| NumberOrString::String(c.to_string())),
            ..Default::default()
        })
        .collect()
}

/// Identifier under / at `offset` (ASCII letters, digits, `_`).
fn word_at(text: &str, offset: usize) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut i = offset.min(bytes.len().saturating_sub(1));
    if !bytes[i].is_ascii_alphanumeric() && bytes[i] != b'_' {
        if i == 0 {
            return None;
        }
        i -= 1;
    }
    if !bytes[i].is_ascii_alphanumeric() && bytes[i] != b'_' {
        return None;
    }
    let mut start = i;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    let mut end = i + 1;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    let name = &text[start..end];
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Applies a single content change to `text`. A change with no range is a full-document
/// replacement; otherwise only the spanned bytes are replaced. Offsets are recomputed per change
/// because changes in one notification apply sequentially.
fn apply_change(text: &mut String, range: Option<Range>, new_text: &str) {
    match range {
        None => *text = new_text.to_string(),
        Some(range) => {
            let line_index = LineIndex::new(text);
            let start = line_index
                .offset(range.start.line, range.start.character)
                .min(text.len());
            let end = line_index
                .offset(range.end.line, range.end.character)
                .min(text.len());
            if start <= end {
                text.replace_range(start..end, new_text);
            }
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        "\"".to_string(),
                        "/".to_string(),
                        "@".to_string(),
                    ]),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: Default::default(),
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: semantic_tokens::TOKEN_TYPES.to_vec(),
                                token_modifiers: vec![],
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        // Register a filesystem watcher so edits to imported `.dream` files (not open in the
        // editor) invalidate cached models and refresh diagnostics of the importers. Failure is
        // non-fatal: clients without dynamic registration just keep the old behavior.
        let _ = self
            .client
            .register_capability(vec![Registration {
                id: "dream-watch-dream-files".to_string(),
                method: "workspace/didChangeWatchedFiles".to_string(),
                register_options: Some(
                    serde_json::json!({ "watchers": [{ "globPattern": "**/*.dream" }] }),
                ),
            }])
            .await;
        self.client
            .log_message(MessageType::INFO, "Dream LSP initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text;
        let version = params.text_document.version;
        self.documents.insert(
            uri.to_string(),
            Document {
                text: text.clone(),
                version,
            },
        );
        self.schedule_diagnostics(uri, text, version);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let version = params.text_document.version;
        let key = uri.to_string();

        let text = {
            let mut entry = self
                .documents
                .entry(key.clone())
                .or_insert_with(|| Document {
                    text: String::new(),
                    version: 0,
                });

            for change in params.content_changes {
                apply_change(&mut entry.text, change.range, &change.text);
            }
            entry.version = version;
            entry.text.clone()
        };

        self.schedule_diagnostics(uri, text, version);
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.documents.remove(&uri);
        self.index_cache.remove(&uri);
        self.pending_diagnostics.remove(&uri);
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        use tower_lsp::lsp_types::FileChangeType;
        // A dependency changed on disk. Cached models embed parsed copies of every imported
        // file, so drop them all (they rebuild lazily on the next request) and re-publish
        // diagnostics for every open document.
        let changed: Vec<String> = params
            .changes
            .iter()
            .filter(|c| c.typ != FileChangeType::DELETED)
            .filter_map(|c| Self::file_path_of(&c.uri))
            .filter(|p| p.ends_with(".dream"))
            .collect();
        if changed.is_empty() {
            return;
        }
        self.index_cache.clear();
        *self.workspace_cache.lock().await = None;
        for entry in self.documents.iter() {
            let (uri, text, version) = (
                Url::parse(&entry.key().clone()).ok(),
                entry.text.clone(),
                entry.version,
            );
            drop(entry);
            let Some(uri) = uri else { continue };
            self.schedule_diagnostics(uri, text, version);
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let key = uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        let line_index = LineIndex::new(&text);
        let offset = line_index.offset(
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character,
        );
        let Some((idx, sema)) = self.models_for(&key, Self::file_path_of(&uri).as_deref()) else {
            return Ok(None);
        };
        if let Some(located) = idx.hover(&text, offset) {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: located.contents,
                }),
                range: Some(Range {
                    start: map_position(line_index.position(located.start)),
                    end: map_position(line_index.position(located.end)),
                }),
            }));
        }
        // The AST index only resolves receivers it could type heuristically; the analyzer's
        // snapshot covers chained/call-result/tuple positions it cannot.
        if let Some(snapshot) = &sema {
            if let Some((start, end, contents)) = crate::sema_ide::hover_at(snapshot, offset) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: contents,
                    }),
                    range: Some(Range {
                        start: map_position(line_index.position(start)),
                        end: map_position(line_index.position(end)),
                    }),
                }));
            }
        }
        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let key = uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        let line_index = LineIndex::new(&text);
        let offset = line_index.offset(
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character,
        );
        let Some((idx, sema)) = self.models_for(&key, Self::file_path_of(&uri).as_deref()) else {
            return Ok(None);
        };
        let sema_loc = sema
            .as_ref()
            .and_then(|s| crate::sema_ide::definition_at(s, &idx, offset))
            .map(|(start, end)| (start, end, None::<String>));
        if let Some((start, end, file_path)) = idx.definition(offset).or(sema_loc) {
            return Ok(
                Self::location_at(&uri, &text, start, end, file_path.as_deref())
                    .map(GotoDefinitionResponse::Scalar),
            );
        }
        Ok(None)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let key = uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        let file_path = Self::file_path_of(&uri);
        let line_index = LineIndex::new(&text);
        let offset = line_index.offset(
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        );
        let Some((idx, sema)) = self.models_for(&key, file_path.as_deref()) else {
            return Ok(None);
        };

        let include_decl = params.context.include_declaration;
        let mut locations: Vec<Location> = Vec::new();

        // Cross-document matches first (other open documents), so the primary document's
        // entries keep the legacy ordering role below.
        if let Some(snapshot) = &sema {
            if let Some(r) = snapshot.ref_covering(offset) {
                if !matches!(
                    r.target,
                    dream_sema::analyzer::ide::IdeTarget::Local { .. }
                        | dream_sema::analyzer::ide::IdeTarget::Expr
                ) {
                    for other_key in self.documents.iter().map(|e| e.key().clone()) {
                        if other_key == key {
                            continue;
                        }
                        let Some(other_uri) = Url::parse(&other_key).ok() else {
                            continue;
                        };
                        let Some((_, other_sema)) =
                            self.models_for(&other_key, Self::file_path_of(&other_uri).as_deref())
                        else {
                            continue;
                        };
                        let Some(other_sema) = other_sema else {
                            continue;
                        };
                        let Some(other_text) = self.document_text(&other_key) else {
                            continue;
                        };
                        let other_li = LineIndex::new(&other_text);
                        for (start, end) in crate::sema_ide::references_in(&other_sema, &r.target) {
                            locations.push(Location {
                                uri: other_uri.clone(),
                                range: Range {
                                    start: map_position(other_li.position(start)),
                                    end: map_position(other_li.position(end)),
                                },
                            });
                        }
                    }
                }
            }
        }

        // This document: sema-precise spans when available, legacy name-based otherwise.
        let spans = match sema
            .as_ref()
            .and_then(|s| Self::precise_references(&idx, s, offset))
        {
            Some((_, mut spans)) => {
                if !include_decl {
                    // Drop the declaration span (the one that is a declaration, not a use).
                    if let Some(snapshot) = &sema {
                        if let Some((ds, de)) =
                            crate::sema_ide::definition_at(snapshot, &idx, offset)
                        {
                            spans.retain(|&(st, en)| (st, en) != (ds, de));
                        }
                    }
                }
                spans
            }
            None => idx.references(offset, include_decl),
        };
        for (start, end) in spans {
            locations.push(Location {
                uri: uri.clone(),
                range: Range {
                    start: map_position(line_index.position(start)),
                    end: map_position(line_index.position(end)),
                },
            });
        }
        Ok(Some(locations))
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let key = uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        let line_index = LineIndex::new(&text);
        let offset = line_index.offset(
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character,
        );
        let Some((idx, sema)) = self.models_for(&key, Self::file_path_of(&uri).as_deref()) else {
            return Ok(None);
        };
        // When the analyzer resolved this position to a cross-file-capable entity, its
        // receiver-typed matching is strictly more precise than the index's name-based match
        // (which collides across same-named members of different types).
        let highlights_spans = sema
            .as_ref()
            .and_then(|s| Self::precise_references(&idx, s, offset))
            .map(|(_, spans)| spans)
            .unwrap_or_else(|| idx.references(offset, true));
        let highlights = highlights_spans
            .into_iter()
            .map(|(start, end)| DocumentHighlight {
                range: Range {
                    start: map_position(line_index.position(start)),
                    end: map_position(line_index.position(end)),
                },
                kind: Some(DocumentHighlightKind::TEXT),
            })
            .collect::<Vec<_>>();
        Ok(Some(highlights))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri.clone();
        let key = uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        let line_index = LineIndex::new(&text);
        let offset = line_index.offset(params.position.line, params.position.character);
        let Some(idx) = self.index_for(&key, Self::file_path_of(&uri).as_deref()) else {
            return Ok(None);
        };
        let Some(decl) = idx.decl_for_offset(offset) else {
            return Ok(None);
        };
        // Only rename symbols whose declaration lives in this document.
        if !decl.is_main || decl.file_path.is_some() {
            return Ok(None);
        }
        if decl.name == "this" || decl.name.is_empty() {
            return Ok(None);
        }
        // Prefer the identifier under the cursor (ref or decl span).
        let (start, end) = idx
            .references(offset, true)
            .into_iter()
            .find(|(s, e)| *s <= offset && offset <= *e)
            .unwrap_or((decl.start, decl.end));
        Ok(Some(PrepareRenameResponse::Range(Range {
            start: map_position(line_index.position(start)),
            end: map_position(line_index.position(end)),
        })))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let key = uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        let line_index = LineIndex::new(&text);
        let offset = line_index.offset(
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        );
        let Some((idx, sema)) = self.models_for(&key, Self::file_path_of(&uri).as_deref()) else {
            return Ok(None);
        };
        let Some(decl) = idx.decl_for_offset(offset) else {
            return Ok(None);
        };
        if !decl.is_main || decl.file_path.is_some() {
            return Ok(None);
        }
        if decl.name == "this" || decl.name.is_empty() {
            return Ok(None);
        }
        let new_name = params.new_name;
        if new_name.is_empty()
            || !new_name
                .chars()
                .next()
                .map(|c| c == '_' || c.is_ascii_alphabetic())
                .unwrap_or(false)
            || !new_name
                .chars()
                .all(|c| c == '_' || c.is_ascii_alphanumeric())
        {
            return Err(jsonrpc::Error {
                code: jsonrpc::ErrorCode::InvalidParams,
                message: "Invalid identifier for rename".into(),
                data: None,
            });
        }

        // Resolve the target once, then collect edits per open document. Sema-resolved targets
        // (fields/methods/enum members/globals) rename across every open document that uses
        // them, matched by entity identity rather than bare name — renaming `Point.x` never
        // touches `Size.x`. Locals keep the exact single-document scope behavior.
        let mut changes: std::collections::HashMap<Url, Vec<TextEdit>> =
            std::collections::HashMap::new();
        let mut push_edits = |uri_key: &str, spans: Vec<(usize, usize)>| {
            if spans.is_empty() {
                return;
            }
            let Ok(doc_uri) = Url::parse(uri_key) else {
                return;
            };
            let Some(doc_text) = self.document_text(uri_key) else {
                return;
            };
            let doc_li = LineIndex::new(&doc_text);
            let edits = spans
                .into_iter()
                .map(|(start, end)| TextEdit {
                    range: Range {
                        start: map_position(doc_li.position(start)),
                        end: map_position(doc_li.position(end)),
                    },
                    new_text: new_name.clone(),
                })
                .collect();
            changes.insert(doc_uri, edits);
        };

        let target_and_spans = sema
            .as_ref()
            .and_then(|s| Self::precise_references(&idx, s, offset));
        if let Some((target, _)) = &target_and_spans {
            for entry in self.documents.iter() {
                let other_key: String = entry.key().clone();
                drop(entry);
                let Some(other_uri) = Url::parse(&other_key).ok().filter(|u| *u != uri) else {
                    continue;
                };
                let Some((_, other_sema)) =
                    self.models_for(&other_key, Self::file_path_of(&other_uri).as_deref())
                else {
                    continue;
                };
                let Some(other_sema) = other_sema else {
                    continue;
                };
                let spans = crate::sema_ide::references_in(&other_sema, target);
                push_edits(&other_key, spans);
            }
        }
        let own_spans = target_and_spans
            .map(|(_, spans)| spans)
            .unwrap_or_else(|| idx.references(offset, true));
        push_edits(&key, own_spans);

        if changes.is_empty() {
            return Ok(None);
        }
        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.clone();
        let key = uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        let line_index = LineIndex::new(&text);
        let Some(idx) = self.index_for(&key, Self::file_path_of(&uri).as_deref()) else {
            return Ok(None);
        };
        let symbols = idx
            .document_symbols()
            .into_iter()
            .map(|d| {
                let range = Range {
                    start: map_position(line_index.position(d.start)),
                    end: map_position(line_index.position(d.end)),
                };
                // `DocumentSymbol::deprecated` is a deprecated field in the external `lsp-types`
                // crate; we must still initialize it, so the allow is unavoidable (not our API).
                #[allow(deprecated)]
                DocumentSymbol {
                    name: d.name.clone(),
                    detail: Some(d.detail.clone()),
                    kind: symbol_kind(d.kind),
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                }
            })
            .collect();
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = params.query;
        let mut out = Vec::new();

        // 1) Every currently-open document (its in-editor version is authoritative). Each
        // document's index is version-cached, so repeated lookups are cheap.
        let keys: Vec<String> = self.documents.iter().map(|e| e.key().clone()).collect();
        for key in &keys {
            let Ok(uri) = Url::parse(key) else {
                continue;
            };
            let Some(text) = self.document_text(key) else {
                continue;
            };
            let Some(idx) = self.index_for(key, Self::file_path_of(&uri).as_deref()) else {
                continue;
            };
            let line_index = LineIndex::new(&text);
            for d in idx.symbols_matching(&query) {
                let range = Range {
                    start: map_position(line_index.position(d.start)),
                    end: map_position(line_index.position(d.end)),
                };
                // `SymbolInformation::deprecated` is a deprecated field in `lsp-types` that must
                // still be initialized; the allow is unavoidable (not our API).
                #[allow(deprecated)]
                out.push(SymbolInformation {
                    name: d.name.clone(),
                    kind: symbol_kind(d.kind),
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri: uri.clone(),
                        range,
                    },
                    container_name: None,
                });
            }
        }

        // 2) Files on disk under the project root (skipping files already open above). The scan
        // is cached briefly and invalidated by file-watch events.
        let open_paths: Vec<String> = keys
            .iter()
            .filter_map(|k| Url::parse(k).ok())
            .filter_map(|u| Self::file_path_of(&u))
            .collect();
        if !open_paths.is_empty() {
            if let Some(root) = crate::workspace::project_root(&open_paths) {
                let mut cache = self.workspace_cache.lock().await;
                let fresh = cache.as_ref().is_some_and(|w| w.is_fresh(&root));
                if !fresh {
                    let symbols = crate::workspace::scan(&root);
                    *cache = Some(crate::workspace::WorkspaceIndex::new(root, symbols));
                }
                if let Some(index) = cache.as_ref() {
                    let open_set: std::collections::HashSet<&String> = open_paths.iter().collect();
                    let lower_query = query.to_lowercase();
                    for s in &index.symbols {
                        if open_set.contains(&s.path) {
                            continue;
                        }
                        if !s.name.to_lowercase().contains(&lower_query) {
                            continue;
                        }
                        let Ok(path) = std::path::PathBuf::from(&s.path).canonicalize() else {
                            continue;
                        };
                        let Some(text) = std::fs::read_to_string(&path).ok() else {
                            continue;
                        };
                        let Some(uri) = Url::from_file_path(&path).ok() else {
                            continue;
                        };
                        let line_index = LineIndex::new(&text);
                        #[allow(deprecated)]
                        out.push(SymbolInformation {
                            name: s.name.clone(),
                            kind: symbol_kind(s.kind),
                            tags: None,
                            deprecated: None,
                            location: Location {
                                uri,
                                range: Range {
                                    start: map_position(line_index.position(s.start)),
                                    end: map_position(line_index.position(s.end)),
                                },
                            },
                            container_name: None,
                        });
                    }
                }
            }
        }

        Ok(Some(out))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri.clone();
        let key = uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        let line_index = LineIndex::new(&text);
        let Some(idx) = self.index_for(&key, Self::file_path_of(&uri).as_deref()) else {
            return Ok(None);
        };

        let mut hints = Vec::new();
        for hint in &idx.inlay_hints {
            let pos = line_index.position(hint.offset);
            // Type hints (`: int`) sit after the name with left padding; parameter-name hints
            // (`x:`) sit before the argument with right padding.
            let (kind, padding_left, padding_right) = match hint.kind {
                index::InlayKind::Type => (InlayHintKind::TYPE, Some(true), None),
                index::InlayKind::Parameter => (InlayHintKind::PARAMETER, None, Some(true)),
            };
            hints.push(InlayHint {
                position: map_position(pos),
                label: InlayHintLabel::String(hint.label.clone()),
                kind: Some(kind),
                text_edits: None,
                tooltip: None,
                padding_left,
                padding_right,
                data: None,
            });
        }
        Ok(Some(hints))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let key = uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        let file_path = Self::file_path_of(&uri);
        let line_index = LineIndex::new(&text);
        let offset = line_index.offset(
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        );
        let Some((idx, sema)) = self.models_for(&key, file_path.as_deref()) else {
            return Ok(None);
        };

        // Member completion after `.`: the analyzer's snapshot resolves the receiver's real
        // Member completion after `.`: the analyzer's snapshot resolves the receiver's real
        // type (locals, chained calls, call results, tuples, loop variables), so it leads the
        // list; AST-index heuristic items are appended for anything lazy instantiation hasn't
        // materialized yet (e.g. `extend T[]` methods never called in this document).
        let mut completions = idx.completions(file_path.as_deref(), &text, offset);
        if index::is_member_completion_context(&text, offset) {
            if let Some(snapshot) = &sema {
                if let Some(items) = crate::sema_ide::member_completions(snapshot, &text, offset) {
                    let seen: std::collections::HashSet<String> =
                        completions.iter().map(|(n, ..)| n.clone()).collect();
                    completions.extend(items.into_iter().filter(|(n, ..)| !seen.contains(n)));
                }
            }
        }
        let import_replace = index::import_path_partial(&text, offset).map(|(start, _)| start);
        let in_attr_name = index::attribute_name_partial(&text, offset).is_some();
        let in_attr_args = index::attribute_arg_context(&text, offset).is_some();

        let items: Vec<CompletionItem> = {
            let mut items: Vec<CompletionItem> = completions
                .into_iter()
                .map(|(label, kind, detail, doc_comment)| {
                    let text_edit = if kind == index::SymKind::Module {
                        if let Some(start) = import_replace {
                            let start_pos = line_index.position(start);
                            let end_pos = line_index.position(offset);
                            Some(CompletionTextEdit::Edit(TextEdit {
                                range: map_range(crate::position::Range {
                                    start: start_pos,
                                    end: end_pos,
                                }),
                                new_text: label.clone(),
                            }))
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let (insert_text, insert_text_format) = if kind == index::SymKind::Decorator
                        && in_attr_name
                    {
                        if let Some(spec) = dream_abi::attributes::find_spec(&label) {
                            match spec.args {
                                dream_abi::attributes::ArgShape::Args { min, .. } if min > 0 => (
                                    Some(format!("{label}($0)")),
                                    Some(InsertTextFormat::SNIPPET),
                                ),
                                _ => (None, None),
                            }
                        } else {
                            (None, None)
                        }
                    } else if kind == index::SymKind::EnumMember {
                        match index::enum_member_snippet(&label, &detail) {
                            Some(snippet) => (Some(snippet), Some(InsertTextFormat::SNIPPET)),
                            None => (None, None),
                        }
                    } else {
                        (None, None)
                    };
                    CompletionItem {
                        label,
                        kind: Some(completion_kind(kind)),
                        detail: Some(detail),
                        documentation: doc_comment.map(|doc| {
                            Documentation::MarkupContent(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: doc,
                            })
                        }),
                        text_edit,
                        insert_text,
                        insert_text_format,
                        ..Default::default()
                    }
                })
                .collect();

            // Offer not-yet-imported stdlib exports with an import edit on accept.
            // Skip inside `import …` (package paths), after `.` (member access — `System.`
            // must not mix in `List` / `Gpu` from unloaded packages), and in `@…` attribute
            // name/arg context.
            if import_replace.is_none()
                && !index::is_member_completion_context(&text, offset)
                && !index::is_switch_arm_completion_context(&text, offset)
                && !in_attr_name
                && !in_attr_args
            {
                let existing: std::collections::HashSet<String> =
                    items.iter().map(|i| i.label.clone()).collect();
                for (label, package, detail) in
                    crate::code_actions::unloaded_import_completions(&text, file_path.as_deref())
                {
                    if existing.contains(&label) {
                        continue;
                    }
                    let additional = crate::code_actions::import_text_edits(&text, &package);
                    items.push(CompletionItem {
                        label,
                        kind: Some(CompletionItemKind::CLASS),
                        detail: Some(detail),
                        additional_text_edits: additional,
                        ..Default::default()
                    });
                }
            }
            items
        };
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();
        let key = uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        let file_path = Self::file_path_of(&uri);
        let mut actions = Vec::new();
        for diag in &params.context.diagnostics {
            let is_unresolved = diag
                .code
                .as_ref()
                .map(|c| matches!(c, NumberOrString::String(s) if s == "unresolved-name"))
                .unwrap_or(false)
                || diag.message.contains("does not exist")
                || diag.message.contains("not found");
            if !is_unresolved {
                continue;
            }
            if let Some(name) = crate::code_actions::unresolved_name_from_message(&diag.message) {
                actions.extend(crate::code_actions::auto_import_actions(
                    &uri,
                    &text,
                    &name,
                    file_path.as_deref(),
                ));
            }
        }
        // Also offer based on the word under the selection range when diagnostics are empty.
        if actions.is_empty() {
            let line_index = LineIndex::new(&text);
            let offset = line_index.offset(params.range.start.line, params.range.start.character);
            if let Some(name) = word_at(&text, offset) {
                actions.extend(crate::code_actions::auto_import_actions(
                    &uri,
                    &text,
                    &name,
                    file_path.as_deref(),
                ));
            }
        }
        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let key = uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        let line_index = LineIndex::new(&text);
        let offset = line_index.offset(
            params.text_document_position_params.position.line,
            params.text_document_position_params.position.character,
        );
        let Some(idx) = self.index_for(&key, Self::file_path_of(&uri).as_deref()) else {
            return Ok(None);
        };
        if let Some(decl) = idx.signature_help(&text, offset) {
            let label = decl.detail.clone();
            let mut parameters = vec![];

            if let Some(start_paren) = label.find('(') {
                if let Some(end_paren) = label.rfind(')') {
                    if start_paren < end_paren {
                        let params_str = &label[start_paren + 1..end_paren];
                        if !params_str.trim().is_empty() {
                            for param in params_str.split(',') {
                                parameters.push(ParameterInformation {
                                    label: ParameterLabel::Simple(param.trim().to_string()),
                                    documentation: None,
                                });
                            }
                        }
                    }
                }
            }

            let active_parameter = active_parameter_at(&text, offset);

            return Ok(Some(SignatureHelp {
                signatures: vec![SignatureInformation {
                    label,
                    documentation: decl.doc_comment.map(|doc| {
                        Documentation::MarkupContent(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: doc,
                        })
                    }),
                    parameters: Some(parameters),
                    active_parameter: Some(active_parameter),
                }],
                active_signature: Some(0),
                active_parameter: Some(active_parameter),
            }));
        }
        Ok(None)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let key = params.text_document.uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        Ok(crate::format::formatting_edits(&text))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.clone();
        let key = uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        // Serve from the cached index (semantic tokens consult the symbol model); building a
        // fresh Index here would re-parse the document + all imports on every token request.
        let Some((idx, _)) = self.models_for(&key, Self::file_path_of(&uri).as_deref()) else {
            return Ok(None);
        };
        let tokens = semantic_tokens::compute_cached(&idx, &text);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri.clone();
        let key = uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        let file_path = Self::file_path_of(&uri);
        if file_path.as_deref().is_some_and(workspace_is_lib_package) {
            return Ok(Some(Vec::new()));
        }
        let line_index = LineIndex::new(&text);
        let Some(idx) = self.index_for(&key, file_path.as_deref()) else {
            return Ok(None);
        };

        let mut lenses = Vec::new();
        // Look for a top-level function named "main"
        for decl in &idx.decls {
            if decl.name == "main" && decl.kind == index::SymKind::Function {
                // The range points to the start of the 'fun main' token
                let range = Range {
                    start: map_position(line_index.position(decl.start)),
                    end: map_position(line_index.position(decl.end)),
                };

                // Add Run CodeLens — extension routes to `dreamer run` when a dream.toml exists.
                lenses.push(CodeLens {
                    range,
                    command: Some(Command {
                        title: "▶ Run".to_string(),
                        command: "dream.runFile".to_string(),
                        arguments: Some(vec![serde_json::json!(uri.to_string())]),
                    }),
                    data: None,
                });

                // Add Debug CodeLens
                lenses.push(CodeLens {
                    range,
                    command: Some(Command {
                        title: "▶ Debug".to_string(),
                        command: "dream.debugFile".to_string(),
                        arguments: Some(vec![serde_json::json!(uri.to_string())]),
                    }),
                    data: None,
                });
            }
        }

        Ok(Some(lenses))
    }
}

/// Counts the comma-separated argument the cursor sits in, by scanning back to the opening paren
/// of the current call (skipping nested parens). Used to highlight the active parameter.
fn active_parameter_at(text: &str, offset: usize) -> u32 {
    let bytes = text.as_bytes();
    let mut active_parameter = 0;
    let mut i = offset;
    let mut paren_count = 0;
    while i > 0 {
        i -= 1;
        let b = bytes[i];
        if b == b')' {
            paren_count += 1;
        } else if b == b'(' {
            if paren_count > 0 {
                paren_count -= 1;
            } else {
                break;
            }
        } else if b == b',' && paren_count == 0 {
            active_parameter += 1;
        } else if b == b';' || b == b'{' || b == b'}' {
            break;
        }
    }
    active_parameter
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Range};

    #[test]
    fn test_apply_change_full_document() {
        let mut text = "hello world".to_string();
        apply_change(&mut text, None, "goodbye");
        assert_eq!(text, "goodbye");
    }

    #[test]
    fn test_apply_change_incremental() {
        let mut text = "hello world\nnew line".to_string();
        // Replace "world" with "there"
        let range = Range {
            start: Position {
                line: 0,
                character: 6,
            },
            end: Position {
                line: 0,
                character: 11,
            },
        };
        apply_change(&mut text, Some(range), "there");
        assert_eq!(text, "hello there\nnew line");
    }

    #[test]
    fn test_apply_change_multi_line() {
        let mut text = "line 1\nline 2\nline 3".to_string();
        // Replace from end of line 1 to start of line 3
        let range = Range {
            start: Position {
                line: 0,
                character: 6,
            },
            end: Position {
                line: 2,
                character: 0,
            },
        };
        apply_change(&mut text, Some(range), " inserted ");
        assert_eq!(text, "line 1 inserted line 3");
    }
}
