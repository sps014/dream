//! Type-aware IDE queries served from the analyzer's [`IdeSnapshot`] (see `dream_sema::analyzer::ide`).
//!
//! The AST index (`index/`) answers "what did the user write"; this module answers "what did the
//! compiler resolve it to". Receiver resolution scans the source bytes back from the cursor to
//! find the token the user just dotted, then looks that span up in the snapshot's reference
//! table — which was populated during real semantic analysis, so chained receivers, call
//! results, tuple elements, and loop variables all resolve exactly as the compiler sees them.

use dream_sema::analyzer::ide::{
    IdeRef, IdeSnapshot, IdeTarget, MemberInfo, MemberKind, TypeSummary,
};

use crate::index::{is_ident_byte, Index, SymKind};

/// Finds the reference recorded for the receiver in a `receiver.<cursor>` completion at `offset`.
///
/// Walks back over the partial identifier, the `.`, and then the receiver token — which may be a
/// plain identifier, or the callee name of a call (`get_list().|`, `obj.method().|`). Returns
/// `None` when the text shape doesn't match a member access or the analyzer recorded nothing for
/// that span (e.g. mid-typing inside an incomplete expression).
pub fn receiver_ref_at(snapshot: &IdeSnapshot, text: &str, offset: usize) -> Option<IdeRef> {
    let bytes = text.as_bytes();
    let offset = offset.min(bytes.len());

    // Scan back over the partial member identifier being completed.
    let mut i = offset;
    while i > 0 && is_ident_byte(bytes[i - 1]) {
        i -= 1;
    }
    if i == 0 || bytes[i - 1] != b'.' {
        return None;
    }
    // Skip whitespace between the dot and the receiver.
    let mut j = i - 1;
    while j > 0 && bytes[j - 1] == b' ' {
        j -= 1;
    }
    let recv_end = j;

    // Case 1: plain identifier receiver (`obj.`).
    let mut start = recv_end;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    if start < recv_end {
        return snapshot.ref_at(start, recv_end).cloned();
    }

    // Case 2: call-result receiver (`f().|`): balance parens back to `(`, then take the callee
    // name token immediately before it. The snapshot records calls at the callee name span.
    if recv_end > 0 && bytes[recv_end - 1] == b')' {
        let mut depth = 0i32;
        let mut k = recv_end;
        while k > 0 {
            match bytes[k - 1] {
                b')' => depth += 1,
                b'(' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            k -= 1;
        }
        if depth == 0 && k > 1 {
            let mut s = k - 1;
            while s > 0 && is_ident_byte(bytes[s - 1]) {
                s -= 1;
            }
            if s < k - 1 {
                return snapshot.ref_at(s, k - 1).cloned();
            }
        }
    }

    None
}

fn member_sym_kind(m: &MemberInfo) -> SymKind {
    if m.name == "await" {
        return SymKind::Keyword;
    }
    match m.kind {
        MemberKind::Field | MemberKind::Property => SymKind::Field,
        MemberKind::Method => SymKind::Method,
        MemberKind::EnumVariant | MemberKind::UnionVariant => SymKind::EnumMember,
    }
}

/// One completion proposal, mirroring the AST-index query output shape.
pub type CompletionOut = (String, SymKind, String, Option<String>);

/// Completions for `receiver.<cursor>` resolved through the analyzer's types. Returns `None`
/// when the receiver's type is unknown (caller falls back to the AST-index heuristic).
pub fn member_completions(
    snapshot: &IdeSnapshot,
    text: &str,
    offset: usize,
) -> Option<Vec<CompletionOut>> {
    let r = receiver_ref_at(snapshot, text, offset)?;

    let members: Vec<MemberInfo> = match &r.result {
        TypeSummary::Tuple { elems } => {
            // Positional access `t.0` / `t.1` / … (the only members a tuple has).
            elems
                .iter()
                .enumerate()
                .map(|(idx, elem)| MemberInfo {
                    kind: MemberKind::Field,
                    name: idx.to_string(),
                    detail: format!("{idx}: {}", elem.display()),
                    is_static: false,
                })
                .collect()
        }
        TypeSummary::Named { ty, .. } => {
            let mut members = snapshot.members_of(*ty);
            if snapshot.future_types.contains(ty) {
                members.insert(
                    0,
                    MemberInfo {
                        kind: MemberKind::Property,
                        name: "await".to_string(),
                        detail: "await".to_string(),
                        is_static: false,
                    },
                );
            }
            members
        }
        TypeSummary::Unknown => return None,
    };

    Some(
        members
            .iter()
            .map(|m| (m.name.clone(), member_sym_kind(m), m.detail.clone(), None))
            .collect(),
    )
}

/// Renders hover markdown for whatever the analyzer resolved at `offset`. Returns `None` when no
/// reference covers the position (caller falls back to the AST-index hover).
pub fn hover_at(snapshot: &IdeSnapshot, offset: usize) -> Option<(usize, usize, String)> {
    let r = snapshot.ref_covering(offset)?;
    let body = hover_body(snapshot, r);
    Some((r.start, r.end, format!("```dream\n{body}\n```")))
}

fn fn_signature(
    snapshot: &IdeSnapshot,
    key: &dream_sema::function_table::FunctionIdentity,
) -> Option<String> {
    let sig = snapshot.functions.get(key)?;
    let params = sig
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.display))
        .collect::<Vec<_>>()
        .join(", ");
    let ret = if sig.ret == "void" {
        String::new()
    } else {
        format!(": {}", sig.ret)
    };
    let prefix = if sig.is_static { "static " } else { "" };
    Some(format!("{prefix}fun {}({params}){ret}", sig.label))
}

fn type_name<'s>(snapshot: &'s IdeSnapshot, ty: dream_types::TypeId, r: &'s IdeRef) -> &'s str {
    snapshot
        .type_names
        .get(&ty)
        .map(String::as_str)
        .unwrap_or_else(|| r.result.display())
}

fn hover_body(snapshot: &IdeSnapshot, r: &IdeRef) -> String {
    match &r.target {
        IdeTarget::Resolved { target, .. } => {
            let mut reference = r.clone();
            reference.target = (**target).clone();
            hover_body(snapshot, &reference)
        }
        IdeTarget::Local { name } | IdeTarget::Global { name } => {
            format!("let {name}: {}", r.result.display())
        }
        IdeTarget::Callee { key, .. } => {
            fn_signature(snapshot, key).unwrap_or_else(|| r.result.display().to_string())
        }
        IdeTarget::Constructor { display, .. } => display.clone(),
        IdeTarget::Field { owner, name } => {
            let ty = snapshot
                .structs
                .get(owner)
                .and_then(|fields| fields.iter().find(|f| &f.name == name))
                .map(|f| f.display.as_str())
                .unwrap_or_else(|| r.result.display());
            format!("{name}: {ty}")
        }
        IdeTarget::EnumMember { owner, member } => {
            let enum_name = type_name(snapshot, *owner, r);
            match snapshot
                .enums
                .get(owner)
                .and_then(|members| members.iter().find(|(n, _)| n == member))
            {
                Some((_, value)) => format!("{enum_name}.{member} = {value}"),
                None => format!("{enum_name}.{member}"),
            }
        }
        IdeTarget::UnionVariant { owner, variant } => {
            let union_key = type_name(snapshot, *owner, r);
            match snapshot.unions.get(owner).and_then(|vs| {
                vs.iter()
                    .find(|v| v.name == *variant)
                    .map(|v| v.fields.clone())
            }) {
                Some(fields) if !fields.is_empty() => {
                    let parts = fields
                        .iter()
                        .map(|f| format!("{}: {}", f.name, f.display))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{union_key}.{variant}({parts})")
                }
                _ => format!("{union_key}.{variant}"),
            }
        }
        IdeTarget::Expr => r.result.display().to_string(),
    }
}

/// Go-to-definition for positions the AST index cannot resolve (chained receivers, call
/// results): maps the analyzer's resolved target back to the indexed declaration.
pub fn definition_at(snapshot: &IdeSnapshot, idx: &Index, offset: usize) -> Option<(usize, usize)> {
    let r = snapshot.ref_covering(offset)?;
    if let IdeTarget::Resolved { source, .. } = &r.target {
        return (source.file == snapshot.primary_file).then_some((source.start, source.end));
    }
    match &r.target {
        IdeTarget::Local { .. } | IdeTarget::Global { .. } => idx
            .decl_for_offset(offset)
            .filter(|decl| decl.is_main && decl.file_path.is_none())
            .map(|decl| (decl.start, decl.end)),
        _ => None,
    }
}

/// True when two resolved targets denote the same program entity — the identity test that makes
/// type-safe references/rename possible (a field named `x` on `Point` does not match a field
/// named `x` on `Size`). Locals deliberately never match across documents: function scopes are
/// not comparable between files.
pub fn target_matches(a: &IdeTarget, b: &IdeTarget) -> bool {
    match (a, b) {
        (IdeTarget::Resolved { source: a, .. }, IdeTarget::Resolved { source: b, .. }) => a == b,
        _ => false,
    }
}

/// True when `r` was recorded in the document whose analysis produced this snapshot. The LSP
/// analyzes the merged program under its synthetic primary-file tag, and imported/prelude code
/// carries real or `<std>/…` paths whose offsets belong to other texts.
pub fn ref_in_primary_doc(r: &IdeRef) -> bool {
    matches!(r.file.as_deref(), None | Some("main.dream"))
}

/// All spans in `snapshot` referencing `target`, restricted to the snapshot's own primary
/// document. Sorted by position.
pub fn references_in(snapshot: &IdeSnapshot, target: &IdeTarget) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = snapshot
        .refs
        .iter()
        .filter(|r| {
            if !ref_in_primary_doc(r) || !target_matches(&r.target, target) {
                return false;
            }
            !matches!(
                &r.target,
                IdeTarget::Resolved { source, .. }
                    if source.file == snapshot.primary_file
                        && (source.start, source.end) == (r.start, r.end)
            )
        })
        .map(|r| (r.start, r.end))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Rename must address the resolved declaration even when the receiver is a chained expression.
pub fn rename_decl_at<'a>(
    snapshot: &IdeSnapshot,
    idx: &'a Index,
    offset: usize,
) -> Option<&'a crate::index::Decl> {
    if let Some(reference) = snapshot.ref_covering(offset) {
        if let IdeTarget::Resolved { source, .. } = &reference.target {
            if source.file != snapshot.primary_file {
                return None;
            }
            return idx.decls.iter().find(|decl| {
                decl.is_main
                    && decl.file_path.is_none()
                    && (decl.start, decl.end) == (source.start, source.end)
            });
        }
        if !matches!(
            reference.target,
            IdeTarget::Local { .. } | IdeTarget::Global { .. }
        ) {
            return None;
        }
    }
    idx.decl_for_offset(offset)
}
