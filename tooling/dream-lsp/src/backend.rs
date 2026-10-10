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
use tower_lsp::{Client, LanguageServer, jsonrpc};

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
        if !matches!(r.target, IdeTarget::Resolved { .. }) {
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
            if let Some(cached) = self.index_cache.get(uri)
                && cached.version == doc.version
            {
                return Some((cached.index.clone(), cached.sema.clone()));
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
            source: Some("dream".to_string()),
            range: map_range(d.range),
            severity: Some(match d.severity {
                "warning" => DiagnosticSeverity::WARNING,
                _ => DiagnosticSeverity::ERROR,
            }),
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

mod completion;
mod formatting;
mod lifecycle;
mod navigation;
mod symbols;

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.handle_initialize(params).await
    }

    async fn initialized(&self, params: InitializedParams) {
        self.handle_initialized(params).await
    }

    async fn shutdown(&self) -> Result<()> {
        self.handle_shutdown().await
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.handle_did_open(params).await
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.handle_did_change(params).await
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.handle_did_close(params).await
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        self.handle_did_change_watched_files(params).await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        self.handle_hover(params).await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        self.handle_goto_definition(params).await
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        self.handle_references(params).await
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        self.handle_document_highlight(params).await
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        self.handle_prepare_rename(params).await
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        self.handle_rename(params).await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        self.handle_document_symbol(params).await
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        self.handle_symbol(params).await
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        self.handle_inlay_hint(params).await
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        self.handle_completion(params).await
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        self.handle_code_action(params).await
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        self.handle_signature_help(params).await
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        self.handle_formatting(params).await
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        self.handle_range_formatting(params).await
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        self.handle_semantic_tokens_full(params).await
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        self.handle_code_lens(params).await
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
