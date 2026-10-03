use super::*;

impl Backend {
    pub(super) async fn handle_hover(&self, params: HoverParams) -> Result<Option<Hover>> {
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

    pub(super) async fn handle_goto_definition(
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

    pub(super) async fn handle_references(
        &self,
        params: ReferenceParams,
    ) -> Result<Option<Vec<Location>>> {
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

    pub(super) async fn handle_document_highlight(
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

    pub(super) async fn handle_prepare_rename(
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

    pub(super) async fn handle_rename(
        &self,
        params: RenameParams,
    ) -> Result<Option<WorkspaceEdit>> {
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
}
