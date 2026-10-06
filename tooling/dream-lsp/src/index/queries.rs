//! Read-only queries over the built [`Index`]: hover, go-to-definition, signature help,
//! completion, and the scope/name-resolution helpers they share.

use super::attr_ide::{
    attribute_arg_completions, attribute_arg_context, attribute_hover, attribute_name_completions,
    attribute_name_partial, attribute_signature,
};
use super::detail_belongs_to;
use super::detail_is_static_method;
use super::{
    is_ident_byte, keywords, substitute_method_type_args, substitute_type_param_t, type_base, Decl,
    Index, Located, Ref, SymKind, GLOBAL,
};
use crate::code_actions::imported_packages;
use dream::driver::source_loader::find_dream_packages_dir;
use dream::syntax::nodes::types::CONSTRUCTOR_NAME;
use dream_abi::attributes::find_spec;
use dream_stdlib::{BOOTSTRAP_PACKAGES, STD_PACKAGES};

/// True when `offset` is in a `receiver.` / `receiver.partial` member-access position.
/// Used by the LSP backend to avoid merging unloaded stdlib type completions into
/// member lists (`System.` must not offer `List` / `Map` / …).
pub fn is_member_completion_context(text: &str, offset: usize) -> bool {
    if import_path_partial(text, offset).is_some() {
        return false;
    }
    let bytes = text.as_bytes();
    let offset = offset.min(bytes.len());
    let mut i = offset;
    while i > 0 && is_ident_byte(bytes[i - 1]) {
        i -= 1;
    }
    i > 0 && bytes[i - 1] == b'.'
}

/// True when completion is inside a switch arm pattern / `case` label (enum variants only).
/// Member access inside an arm (`case Color.|`) is not switch-arm context.
pub fn is_switch_arm_completion_context(text: &str, offset: usize) -> bool {
    !is_member_completion_context(text, offset) && switch_arm_subject(text, offset).is_some()
}

/// If `offset` is inside an unquoted `import <path>` statement, returns
/// `(path_start_byte, partial_path)` where `partial_path` is the text from the path start to
/// the cursor (e.g. `""`, `system`, `system.`). Outside import context returns `None` so
/// `System.` member completion is unaffected.
pub(crate) fn import_path_partial(text: &str, offset: usize) -> Option<(usize, String)> {
    let bytes = text.as_bytes();
    let offset = offset.min(bytes.len());

    // Walk back over the partial dotted path.
    let mut path_start = offset;
    while path_start > 0 {
        let b = bytes[path_start - 1];
        if is_ident_byte(b) || b == b'.' {
            path_start -= 1;
        } else {
            break;
        }
    }

    // Skip whitespace between `import` and the path.
    let mut j = path_start;
    while j > 0 && (bytes[j - 1] == b' ' || bytes[j - 1] == b'\t') {
        j -= 1;
    }

    if j < 6 {
        return None;
    }
    if &text[j - 6..j] != "import" {
        return None;
    }
    // Word boundary before `import`.
    if j > 6 {
        let before = bytes[j - 7];
        if is_ident_byte(before) {
            return None;
        }
    }
    // Must be on the same line as `import` (no newline between import and cursor).
    if text[j..offset].contains('\n') {
        return None;
    }

    Some((path_start, text[path_start..offset].to_string()))
}

fn push_module_completion(
    out: &mut Vec<(String, SymKind, String, Option<String>)>,
    name: String,
    detail: &str,
    imported: &std::collections::HashSet<String>,
) {
    if imported.contains(&name) {
        return;
    }
    if out.iter().any(|(n, ..)| n == &name) {
        return;
    }
    out.push((name, SymKind::Module, detail.to_string(), None));
}

/// Package / local-module completions for an unquoted `import` path prefix.
fn import_path_completions(
    file_path: Option<&str>,
    text: &str,
    partial: &str,
) -> Vec<(String, SymKind, String, Option<String>)> {
    let imported = imported_packages(text);
    let mut out = Vec::new();

    for pkg in STD_PACKAGES {
        if BOOTSTRAP_PACKAGES.contains(&pkg.name) {
            continue;
        }
        if pkg.name.starts_with(partial) {
            push_module_completion(&mut out, pkg.name.to_string(), "stdlib package", &imported);
        }
    }

    if let Some(path_str) = file_path {
        let parent_dir = std::path::Path::new(path_str)
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));

        // Local `.dream` files / directories as dotted relative modules.
        if let Ok(entries) = std::fs::read_dir(parent_dir) {
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                let name = entry.file_name().to_string_lossy().to_string();
                if file_type.is_dir() {
                    if name.starts_with('.') || name == "dream_packages" {
                        continue;
                    }
                    if name.starts_with(partial) || format!("{}.", name).starts_with(partial) {
                        push_module_completion(&mut out, name, "directory", &imported);
                    }
                } else if let Some(stem) = name.strip_suffix(".dream")
                    && stem.starts_with(partial) {
                        push_module_completion(&mut out, stem.to_string(), "module", &imported);
                    }
            }
        }

        if let Some(packages_dir) = find_dream_packages_dir(parent_dir)
            && let Ok(entries) = std::fs::read_dir(&packages_dir) {
                for entry in entries.flatten() {
                    if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        continue;
                    }
                    let pkg_name = entry.file_name().to_string_lossy().to_string();
                    // Bare package name: `import sem` / `import |`
                    if pkg_name.starts_with(partial) {
                        push_module_completion(&mut out, pkg_name.clone(), "package", &imported);
                    }
                    // Submodules: `import mathpkg.` / `import mathpkg.op`
                    let pkg_prefix = format!("{}.", pkg_name);
                    if partial.starts_with(&pkg_prefix) || partial == pkg_name {
                        let src_dir = entry.path().join("src");
                        if let Ok(src_entries) = std::fs::read_dir(&src_dir) {
                            for src_entry in src_entries.flatten() {
                                let Some(stem) = src_entry
                                    .file_name()
                                    .to_str()
                                    .and_then(|n| n.strip_suffix(".dream").map(str::to_string))
                                else {
                                    continue;
                                };
                                // Entry file is imported as bare `pkg`, not `pkg.pkg`.
                                if stem == pkg_name {
                                    continue;
                                }
                                let full = format!("{}.{}", pkg_name, stem);
                                if full.starts_with(partial) {
                                    push_module_completion(
                                        &mut out,
                                        full,
                                        "package module",
                                        &imported,
                                    );
                                }
                            }
                        }
                    }
                }
            }
    }

    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

mod completion;
mod navigation;
mod resolution;
impl Index {}

/// Subject expression of the enclosing `switch (…)` when `offset` is in an arm pattern
/// (before `=>`) or a C-style `case` label. Returns `None` in arm bodies / outside switch.
fn switch_arm_subject(text: &str, offset: usize) -> Option<String> {
    let bytes = text.as_bytes();
    let offset = offset.min(bytes.len());

    // Find the `{` that opens the switch body containing `offset`.
    let mut i = offset;
    let mut brace_depth = 0i32;
    let mut body_open = None;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => brace_depth += 1,
            b'{' => {
                if brace_depth > 0 {
                    brace_depth -= 1;
                } else {
                    body_open = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let body_open = body_open?;

    // `switch (…) {` — walk back over `)` and extract the subject, then require `switch`.
    let mut j = body_open;
    while j > 0 && (bytes[j - 1] == b' ' || bytes[j - 1] == b'\t' || bytes[j - 1] == b'\n') {
        j -= 1;
    }
    if j == 0 || bytes[j - 1] != b')' {
        return None;
    }
    let close_paren = j - 1;
    let mut paren_depth = 1i32;
    let mut k = close_paren;
    while k > 0 {
        k -= 1;
        match bytes[k] {
            b')' => paren_depth += 1,
            b'(' => {
                paren_depth -= 1;
                if paren_depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
    if paren_depth != 0 {
        return None;
    }
    let open_paren = k;
    let subject = text[open_paren + 1..close_paren].trim().to_string();
    if subject.is_empty() {
        return None;
    }

    let mut sw = open_paren;
    while sw > 0 && (bytes[sw - 1] == b' ' || bytes[sw - 1] == b'\t' || bytes[sw - 1] == b'\n') {
        sw -= 1;
    }
    if sw < 6 || &text[sw - 6..sw] != "switch" {
        return None;
    }
    if sw > 6 && is_ident_byte(bytes[sw - 7]) {
        return None;
    }

    // From body `{` to cursor: if we're past a `=>` at brace/paren depth 0 of this arm, we're
    // in the arm body — don't offer variants there.
    if arm_slice_past_arrow(&text[body_open + 1..offset]) {
        return None;
    }

    Some(subject)
}

/// True when `slice` (text from switch `{` to cursor) has already crossed a pattern `=>`
/// into the current arm's body.
fn arm_slice_past_arrow(slice: &str) -> bool {
    let bytes = slice.as_bytes();
    let mut paren = 0i32;
    let mut brace = 0i32;
    let mut bracket = 0i32;
    let mut i = 0usize;
    let mut last_arrow = None;
    while i + 1 < bytes.len() {
        match bytes[i] {
            b'(' => paren += 1,
            b')' => paren -= 1,
            b'{' => brace += 1,
            b'}' => brace -= 1,
            b'[' => bracket += 1,
            b']' => bracket -= 1,
            b'=' if paren == 0 && brace == 0 && bracket == 0 && bytes[i + 1] == b'>' => {
                last_arrow = Some(i);
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let Some(arrow) = last_arrow else {
        return false;
    };
    // After `=>`, a comma at depth 0 starts a new arm — if the cursor is after such a comma,
    // we're in the next pattern again.
    let after = &bytes[arrow + 2..];
    let mut paren = 0i32;
    let mut brace = 0i32;
    let mut bracket = 0i32;
    let mut saw_comma = false;
    for &b in after {
        match b {
            b'(' => paren += 1,
            b')' => paren -= 1,
            b'{' => brace += 1,
            b'}' => brace -= 1,
            b'[' => bracket += 1,
            b']' => bracket -= 1,
            b',' if paren == 0 && brace == 0 && bracket == 0 => saw_comma = true,
            _ => {}
        }
    }
    !saw_comma
}

fn switch_arm_is_c_style_case(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let mut i = offset.min(bytes.len());
    while i > 0 && is_ident_byte(bytes[i - 1]) {
        i -= 1;
    }
    while i > 0 && (bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
        i -= 1;
    }
    i >= 4 && &text[i - 4..i] == "case" && (i == 4 || !is_ident_byte(bytes[i - 5]))
}

fn partial_ident_before(text: &str, offset: usize) -> String {
    let bytes = text.as_bytes();
    let mut i = offset.min(bytes.len());
    while i > 0 && is_ident_byte(bytes[i - 1]) {
        i -= 1;
    }
    text[i..offset.min(bytes.len())].to_string()
}

/// True when a method detail declares type parameters before `(`, e.g.
/// `async TaskPool.dispatch<TIn, TOut>(…)`.
fn method_detail_has_type_params(detail: &str) -> bool {
    let Some(paren) = detail.find('(') else {
        return false;
    };
    detail[..paren].rfind('<').is_some()
}

/// Parses call-site method type arguments after a method name token: `dispatch<int, string>(`.
/// `name_end` is the exclusive end offset of the method identifier.
fn method_type_args_at(text: &str, name_end: usize) -> Option<Vec<String>> {
    let bytes = text.as_bytes();
    let mut i = name_end.min(bytes.len());
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'<' {
        return None;
    }
    let start = i + 1;
    let mut depth = 1i32;
    let mut j = start;
    while j < bytes.len() {
        match bytes[j] {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    let inner = text[start..j].trim();
                    if inner.is_empty() {
                        return Some(Vec::new());
                    }
                    return Some(
                        inner
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect(),
                    );
                }
            }
            b'(' if depth == 1 => return None, // malformed
            _ => {}
        }
        j += 1;
    }
    None
}

/// Class type args in `List<float>.alloc` — the `<…>` sits before the `.`, not after the method.
fn type_args_before_member_dot(text: &str, member_start: usize) -> Option<Vec<String>> {
    let bytes = text.as_bytes();
    let mut i = member_start.min(bytes.len());
    while i > 0 && (bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
        i -= 1;
    }
    if i == 0 || bytes[i - 1] != b'.' {
        return None;
    }
    i -= 1;
    while i > 0 && (bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
        i -= 1;
    }
    if i == 0 || bytes[i - 1] != b'>' {
        return None;
    }
    let gt = i - 1;
    let mut depth = 1i32;
    let mut j = gt;
    while j > 0 {
        j -= 1;
        match bytes[j] {
            b'>' => depth += 1,
            b'<' => {
                depth -= 1;
                if depth == 0 {
                    let inner = text[j + 1..gt].trim();
                    if inner.is_empty() {
                        return Some(Vec::new());
                    }
                    return Some(
                        inner
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect(),
                    );
                }
            }
            _ => {}
        }
    }
    None
}
