use super::*;

impl Backend {
    pub(super) async fn handle_completion(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
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
        if index::is_member_completion_context(&text, offset)
            && let Some(snapshot) = &sema
                && let Some(items) = crate::sema_ide::member_completions(snapshot, &text, offset) {
                    let seen: std::collections::HashSet<String> =
                        completions.iter().map(|(n, ..)| n.clone()).collect();
                    completions.extend(items.into_iter().filter(|(n, ..)| !seen.contains(n)));
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
            // must not mix in `List` / `Map` from unloaded packages), and in `@…` attribute
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

    pub(super) async fn handle_code_action(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<CodeActionResponse>> {
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
                .map(|c| {
                    matches!(
                        c,
                        NumberOrString::String(s) if s == "unresolved-name" || s == "missing-member"
                    )
                })
                .unwrap_or(false)
                || diag.message.contains("does not exist")
                || diag.message.contains("not found")
                || diag.message.contains("has no method")
                || diag.message.contains("has no static method");
            if !is_unresolved {
                continue;
            }
            for name in crate::code_actions::unresolved_names_from_message(&diag.message) {
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

    pub(super) async fn handle_signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> Result<Option<SignatureHelp>> {
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

            if let Some(start_paren) = label.find('(')
                && let Some(end_paren) = label.rfind(')')
                    && start_paren < end_paren {
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
}
