use super::*;

impl Backend {
    pub(super) async fn handle_document_symbol(
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

    pub(super) async fn handle_symbol(
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

    pub(super) async fn handle_inlay_hint(
        &self,
        params: InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
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

    pub(super) async fn handle_semantic_tokens_full(
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

    pub(super) async fn handle_code_lens(
        &self,
        params: CodeLensParams,
    ) -> Result<Option<Vec<CodeLens>>> {
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
