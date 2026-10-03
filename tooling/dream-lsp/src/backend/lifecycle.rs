use super::*;

impl Backend {
    pub(super) async fn handle_initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
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
                document_range_formatting_provider: Some(OneOf::Left(true)),
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

    pub(super) async fn handle_initialized(&self, _: InitializedParams) {
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

    pub(super) async fn handle_shutdown(&self) -> Result<()> {
        Ok(())
    }

    pub(super) async fn handle_did_open(&self, params: DidOpenTextDocumentParams) {
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

    pub(super) async fn handle_did_change(&self, params: DidChangeTextDocumentParams) {
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

    pub(super) async fn handle_did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.documents.remove(&uri);
        self.index_cache.remove(&uri);
        self.pending_diagnostics.remove(&uri);
    }

    pub(super) async fn handle_did_change_watched_files(
        &self,
        params: DidChangeWatchedFilesParams,
    ) {
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
}
