use super::*;

impl Index {
    pub fn hover(&self, text: &str, offset: usize) -> Option<Located> {
        let mut receiver_ty_opt = None;
        let (start, end, decl) = if let Some(decl) = self.decl_at(offset) {
            (decl.start, decl.end, decl)
        } else {
            let reference = self.ref_at(offset)?;
            if reference.kind == SymKind::Decorator {
                let spec = find_spec(&reference.name)?;
                return Some(Located {
                    start: reference.start,
                    end: reference.end,
                    contents: attribute_hover(spec),
                });
            }
            let receiver = reference.receiver.as_deref();
            let d = match reference.kind {
                SymKind::EnumMember => self.resolve_enum_member(receiver, &reference.name),
                SymKind::Field | SymKind::Method => {
                    let mut recv_ty = None;
                    if let Some(recv) = receiver {
                        recv_ty = self.receiver_type_name(recv, reference.scope, reference.start);
                        receiver_ty_opt = recv_ty.clone();
                    }
                    self.resolve_member(recv_ty.as_deref(), &reference.name)
                }
                _ => self.resolve(&reference.name, reference.scope, reference.start),
            }?;
            (reference.start, reference.end, d)
        };

        let mut type_args = if decl.kind == SymKind::Method {
            method_type_args_at(text, end).unwrap_or_default()
        } else {
            Vec::new()
        };
        // `GpuBuffer<float>.alloc` puts class args before the `.`, not after the method name.
        if type_args.is_empty() && decl.kind == SymKind::Method {
            if let Some(args) = type_args_before_member_dot(text, start) {
                type_args = args;
            }
        }
        let detail =
            Self::apply_type_args_to_detail(&decl.detail, receiver_ty_opt.as_deref(), &type_args);

        let mut contents = format!("```dream\n{}\n```", detail);
        if let Some(doc) = &decl.doc_comment {
            contents.push_str("\n\n---\n\n");
            contents.push_str(doc);
        }

        Some(Located {
            start,
            end,
            contents,
        })
    }

    /// Resolves the declaration the cursor sits on, whether `offset` lands on the declaration's
    /// own name or on a reference to it. Shared by go-to-definition, find-references, and rename.
    pub fn decl_for_offset(&self, offset: usize) -> Option<&Decl> {
        if let Some(decl) = self.decl_at(offset) {
            return Some(decl);
        }
        let reference = self.ref_at(offset)?;
        match reference.kind {
            SymKind::EnumMember => {
                self.resolve_enum_member(reference.receiver.as_deref(), &reference.name)
            }
            SymKind::Field | SymKind::Method => {
                let recv_ty = reference.receiver.as_ref().and_then(|recv| {
                    self.receiver_type_name(recv, reference.scope, reference.start)
                });
                self.resolve_member(recv_ty.as_deref(), &reference.name)
            }
            _ => self.resolve(&reference.name, reference.scope, reference.start),
        }
    }

    pub fn definition(&self, offset: usize) -> Option<(usize, usize, Option<String>)> {
        self.decl_for_offset(offset)
            .map(|d| (d.start, d.end, d.file_path.clone()))
    }

    /// All occurrences (byte spans) of the symbol under `offset`: the declaration (when
    /// `include_declaration`) plus every recorded reference that resolves to it. Locals and
    /// parameters are confined to their function scope; everything else matches by name across the
    /// document, mirroring the index's best-effort resolution. Spans are always in the open
    /// document; declarations that live in another file are omitted from the declaration slot
    /// (use [`definition`](Self::definition) to navigate there).
    pub fn references(&self, offset: usize, include_declaration: bool) -> Vec<(usize, usize)> {
        let Some(decl) = self.decl_for_offset(offset) else {
            return Vec::new();
        };
        let name = decl.name.clone();
        let is_local =
            matches!(decl.kind, SymKind::Param | SymKind::Variable) && decl.scope != GLOBAL;
        let scope = decl.scope;
        let decl_in_main = decl.is_main && decl.file_path.is_none();
        let decl_span = (decl.start, decl.end);

        let mut out = Vec::new();
        if include_declaration && decl_in_main {
            out.push(decl_span);
        }
        for r in &self.refs {
            if !r.is_main || r.name != name {
                continue;
            }
            if is_local && r.scope != scope {
                continue;
            }
            out.push((r.start, r.end));
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The document's outline: top-level declarations (functions, types, enum members, fields,
    /// methods, and file-scope globals), excluding locals and parameters. Used for the document
    /// symbols / outline view.
    pub fn document_symbols(&self) -> Vec<&Decl> {
        self.decls
            .iter()
            .filter(|d| {
                d.is_main
                    && match d.kind {
                        SymKind::Variable => d.scope == GLOBAL,
                        SymKind::Param | SymKind::Keyword | SymKind::Type | SymKind::Decorator => {
                            false
                        }
                        _ => true,
                    }
            })
            .collect()
    }

    /// Workspace "go to symbol" candidates whose name matches `query` (case-insensitive substring;
    /// an empty query matches every candidate). Uses the same named-declaration filter as
    /// [`document_symbols`](Self::document_symbols) but drops the `scope == GLOBAL` restriction on
    /// variables so any named declaration is discoverable, mirroring how editors surface symbols
    /// across a workspace.
    pub fn symbols_matching(&self, query: &str) -> Vec<&Decl> {
        let needle = query.to_lowercase();
        self.decls
            .iter()
            .filter(|d| {
                d.is_main
                    && !matches!(
                        d.kind,
                        SymKind::Param | SymKind::Keyword | SymKind::Type | SymKind::Decorator
                    )
                    && (needle.is_empty() || d.name.to_lowercase().contains(&needle))
            })
            .collect()
    }

    pub fn signature_help(&self, text: &str, offset: usize) -> Option<Decl> {
        if let Some((spec, _active)) = attribute_signature(text, offset) {
            return Some(Decl {
                name: spec.name.to_string(),
                kind: SymKind::Decorator,
                detail: spec.args.signature_label(spec.name),
                doc_comment: Some(spec.doc.to_string()),
                start: 0,
                end: 0,
                scope: GLOBAL,
                ty: None,
                is_main: true,
                file_path: None,
            });
        }

        let bytes = text.as_bytes();
        let mut i = offset;
        let mut paren_count = 0;
        let mut open_paren_offset = None;

        while i > 0 {
            i -= 1;
            let b = bytes[i];
            if b == b')' {
                paren_count += 1;
            } else if b == b'(' {
                if paren_count > 0 {
                    paren_count -= 1;
                } else {
                    open_paren_offset = Some(i);
                    break;
                }
            } else if b == b';' || b == b'{' || b == b'}' {
                return None;
            }
        }

        let op_idx = open_paren_offset?;
        let mut j = op_idx;
        while j > 0 && (bytes[j - 1] == b' ' || bytes[j - 1] == b'\t' || bytes[j - 1] == b'\n') {
            j -= 1;
        }
        let recv_end = j;
        let mut recv_start = recv_end;
        while recv_start > 0 && is_ident_byte(bytes[recv_start - 1]) {
            recv_start -= 1;
        }

        if recv_start == recv_end {
            return None;
        }

        let name = &text[recv_start..recv_end];
        let scope = self.enclosing_scope(offset);

        let mut k = recv_start;
        while k > 0 && (bytes[k - 1] == b' ' || bytes[k - 1] == b'\t' || bytes[k - 1] == b'\n') {
            k -= 1;
        }
        if k > 0 && bytes[k - 1] == b'.' {
            let mut j2 = k - 1;
            while j2 > 0 && bytes[j2 - 1] == b' ' {
                j2 -= 1;
            }
            let recv_obj_end = j2;
            let mut recv_obj_start = recv_obj_end;
            while recv_obj_start > 0 && is_ident_byte(bytes[recv_obj_start - 1]) {
                recv_obj_start -= 1;
            }
            let receiver_obj = &text[recv_obj_start..recv_obj_end];
            let receiver_ty_opt = self.receiver_type_name(receiver_obj, scope, recv_obj_start);

            if let Some(decl) = self.resolve_member(receiver_ty_opt.as_deref(), name) {
                let mut d = decl.clone();
                let mut type_args = method_type_args_at(text, recv_end).unwrap_or_default();
                if type_args.is_empty() {
                    if let Some(args) = type_args_before_member_dot(text, recv_start) {
                        type_args = args;
                    }
                }
                d.detail = Self::apply_type_args_to_detail(
                    &d.detail,
                    receiver_ty_opt.as_deref(),
                    &type_args,
                );
                return Some(d);
            }
        } else {
            if let Some(decl) = self.resolve(name, scope, recv_start) {
                if matches!(decl.kind, SymKind::Class | SymKind::Struct) {
                    if let Some(ctor_decl) = self.decls.iter().find(|d| {
                        d.name == CONSTRUCTOR_NAME
                            && d.kind == SymKind::Method
                            && detail_belongs_to(&d.detail, name)
                    }) {
                        return Some(ctor_decl.clone());
                    }
                } else {
                    return Some(decl.clone());
                }
            }
            // For struct initializers where `resolve` failed entirely (e.g. static imports sometimes)
            if let Some(decl) = self.decls.iter().find(|d| {
                d.name == CONSTRUCTOR_NAME
                    && d.kind == SymKind::Method
                    && detail_belongs_to(&d.detail, name)
            }) {
                return Some(decl.clone());
            }
        }

        None
    }
}
