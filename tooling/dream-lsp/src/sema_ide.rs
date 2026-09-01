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

use crate::index::{detail_belongs_to, is_ident_byte, type_base, Decl, Index, SymKind};

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
        TypeSummary::Named { key, .. } => {
            let key = key.as_deref()?;
            snapshot.members_of(key)
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

fn fn_signature(snapshot: &IdeSnapshot, key: &str) -> Option<String> {
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
    Some(format!("{prefix}fun {key}({params}){ret}"))
}

fn hover_body(snapshot: &IdeSnapshot, r: &IdeRef) -> String {
    match &r.target {
        IdeTarget::Local { name } | IdeTarget::Global { name } => {
            format!("let {name}: {}", r.result.display())
        }
        IdeTarget::Callee { key, .. } => {
            fn_signature(snapshot, key).unwrap_or_else(|| r.result.display().to_string())
        }
        IdeTarget::Constructor { type_key } => type_key.clone(),
        IdeTarget::Field { type_key, name } => {
            let ty = snapshot
                .structs
                .get(type_key)
                .and_then(|fields| fields.iter().find(|f| &f.name == name))
                .map(|f| f.display.as_str())
                .unwrap_or_else(|| r.result.display());
            format!("{name}: {ty}")
        }
        IdeTarget::EnumMember { enum_name, member } => match snapshot.enums.get(enum_name) {
            Some(members) => match members.iter().find(|(n, _)| n == member) {
                Some((_, value)) => format!("{enum_name}.{member} = {value}"),
                None => format!("{enum_name}.{member}"),
            },
            None => format!("{enum_name}.{member}"),
        },
        IdeTarget::UnionVariant { union_key, variant } => {
            match snapshot.unions.get(union_key).and_then(|vs| {
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
    let decl: Option<&Decl> = match &r.target {
        IdeTarget::Local { name } | IdeTarget::Global { name } => idx
            .decls
            .iter()
            .filter(|d| {
                d.name == *name
                    && matches!(d.kind, SymKind::Variable | SymKind::Param)
                    && d.start <= offset
            })
            .max_by_key(|d| d.start),
        IdeTarget::Field { type_key, name } => idx.decls.iter().find(|d| {
            d.kind == SymKind::Field
                && d.name == *name
                && detail_belongs_to(&d.detail, type_base(type_key))
        }),
        IdeTarget::Callee { key, label } => {
            // Instance/static methods register under `{Type}_{method}`; free functions keep
            // their bare name. Prefer the method interpretation when the key carries a `_`.
            idx.decls
                .iter()
                .find(|d| {
                    d.kind == SymKind::Method && d.name == *label && method_matches_key(d, key)
                })
                .or_else(|| {
                    idx.decls
                        .iter()
                        .find(|d| d.kind == SymKind::Function && d.name == *label)
                })
        }
        IdeTarget::Constructor { type_key } => idx.decls.iter().find(|d| {
            matches!(d.kind, SymKind::Class | SymKind::Struct) && d.name == type_base(type_key)
        }),
        IdeTarget::EnumMember { enum_name, member } => idx.decls.iter().find(|d| {
            d.kind == SymKind::EnumMember
                && d.name == *member
                && d.detail.starts_with(&format!("{enum_name}."))
        }),
        IdeTarget::UnionVariant { union_key, variant } => idx.decls.iter().find(|d| {
            d.kind == SymKind::EnumMember
                && d.name == *variant
                && detail_belongs_to(&d.detail, type_base(union_key))
        }),
        IdeTarget::Expr => None,
    };
    decl.map(|d| (d.start, d.end))
}

fn method_matches_key(decl: &Decl, key: &str) -> bool {
    // The emitted key is `{Type}_{method}[.overloads]` (possibly module-qualified). Mangling
    // appends suffixes to the base name, so the base type is everything before the first `_`.
    let stripped = key.rsplit("::").next().unwrap_or(key);
    let no_overload = stripped.split('.').next().unwrap_or(stripped);
    let base_ty = no_overload.split('_').next().unwrap_or(no_overload);
    detail_belongs_to(&decl.detail, base_ty)
}

/// True when two resolved targets denote the same program entity — the identity test that makes
/// type-safe references/rename possible (a field named `x` on `Point` does not match a field
/// named `x` on `Size`). Locals deliberately never match across documents: function scopes are
/// not comparable between files.
pub fn target_matches(a: &IdeTarget, b: &IdeTarget) -> bool {
    match (a, b) {
        (IdeTarget::Global { name: a }, IdeTarget::Global { name: b }) => a == b,
        (
            IdeTarget::Field {
                type_key: ta,
                name: fa,
            },
            IdeTarget::Field {
                type_key: tb,
                name: fb,
            },
        ) =>
        // Mangling appends generic suffixes to the base name; compare bases.
        {
            fa == fb && type_base(ta) == type_base(tb)
        }
        (IdeTarget::Callee { key: a, .. }, IdeTarget::Callee { key: b, .. }) => {
            // Emitted keys encode the declaring type + overload signature exactly.
            a.rsplit("::").next() == b.rsplit("::").next()
        }
        (
            IdeTarget::EnumMember {
                enum_name: ea,
                member: ma,
            },
            IdeTarget::EnumMember {
                enum_name: eb,
                member: mb,
            },
        ) => ea == eb && ma == mb,
        (
            IdeTarget::UnionVariant {
                union_key: ua,
                variant: va,
            },
            IdeTarget::UnionVariant {
                union_key: ub,
                variant: vb,
            },
        ) => type_base(ua) == type_base(ub) && va == vb,
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
        .filter(|r| ref_in_primary_doc(r) && target_matches(&r.target, target))
        .map(|r| (r.start, r.end))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}
