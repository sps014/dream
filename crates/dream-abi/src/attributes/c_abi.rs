//! The `@c` family: C ABI bindings, string encoding, and packing.

use super::*;

pub const C_ATTR: &str = "c";

/// Extracts the `(lib, symbol)` pair from `@c("lib", "symbol")`, or `None` when absent.
pub fn c_import_target(attributes: &[AttributeNode]) -> Option<(String, String)> {
    let c = attributes
        .iter()
        .find(|a| is_compiler_attribute(a, C_ATTR))?;
    let lib = c.args.first()?.as_string()?.to_string();
    let symbol = c.args.get(1)?.as_string()?.to_string();
    Some((lib, symbol))
}

/// True when the declaration carries `@c`.
pub fn has_c_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, C_ATTR)
}

/// True when a value struct carries `@packed`.
pub fn has_packed_attr(attributes: &[AttributeNode]) -> bool {
    attributes
        .iter()
        .any(|a| is_compiler_attribute(a, "packed"))
}

/// How a `@c` extern exchanges `string`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringEncoding {
    /// A null-terminated `const char*` of UTF-8.
    Utf8,
    /// A null-terminated `const uint16_t*` (`char16_t*` / `LPWSTR`, not `wchar_t`).
    Utf16,
}

/// Where a `NativeCallback` parameter places `user_data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserDataPosition {
    /// `user_data` is the first C argument.
    First,
    /// `user_data` is the last C argument.
    Last,
}

/// The `@marshal` encoding. An extern without the attribute uses UTF-8.
#[must_use]
pub fn string_encoding(attributes: &[AttributeNode]) -> StringEncoding {
    match attributes
        .iter()
        .find(|a| is_compiler_attribute(a, "marshal"))
        .and_then(enum_member)
    {
        Some("Utf16") => StringEncoding::Utf16,
        _ => StringEncoding::Utf8,
    }
}

/// The `@user_data` position, when the parameter declares one.
#[must_use]
pub fn user_data_position(attributes: &[AttributeNode]) -> Option<UserDataPosition> {
    match attributes
        .iter()
        .find(|a| is_compiler_attribute(a, "user_data"))
        .and_then(enum_member)?
    {
        "Last" => Some(UserDataPosition::Last),
        "First" => Some(UserDataPosition::First),
        _ => None,
    }
}

fn enum_member(attr: &AttributeNode) -> Option<&str> {
    let AttributeArg::Enum(path) = attr.args.first()? else {
        return None;
    };
    Some(&path.last()?.text)
}

/// Reports `@c`-family placement errors on one extern's attribute list:
/// - `@c` combined with `@js` / `@runtime` / `@intrinsic` (incompatible binding hosts),
/// - `@runtime` combined with `@js` / `@c` / `@intrinsic`,
/// - `@marshal(...)` without `@c` (only meaningful for the C ABI).
///
/// Call after generic attribute shape validation.
pub fn validate_c_extern_attrs(attrs: &[AttributeNode], diagnostics: &mut DiagnosticBag) {
    if extern_binding_conflict(attrs) {
        let pos = attrs
            .iter()
            .find(|a| {
                [
                    C_ATTR,
                    "js",
                    "runtime",
                    crate::intrinsics::INTRINSIC_ATTR,
                    crate::intrinsics::LLVM_ATTR,
                ]
                .iter()
                .any(|name| is_compiler_attribute(a, name))
            })
            .map(|a| a.name.position);
        diagnostics.report_error(
            "an extern function cannot combine `@c`, `@js`, `@runtime`, `@intrinsic`, or `@llvm`"
                .to_string(),
            pos,
        );
    }
    if !has_c_attr(attrs)
        && let Some(attr) = attrs.iter().find(|a| is_compiler_attribute(a, "marshal"))
    {
        diagnostics.report_error(
            "'@marshal' requires '@c' on the same extern (it only applies to C ABI imports)"
                .to_string(),
            Some(attr.name.position),
        );
    }
}

/// `@user_data` names a known position. Placement on a parameter is the attribute's target.
pub fn validate_user_data(attrs: &[AttributeNode], diagnostics: &mut DiagnosticBag) {
    let Some(attr) = attrs.iter().find(|a| is_compiler_attribute(a, "user_data")) else {
        return;
    };
    if !matches!(enum_member(attr), Some("First" | "Last")) {
        diagnostics.report_error(
            "'@user_data' must be `UserDataPosition.First` or `UserDataPosition.Last`".to_string(),
            Some(attr.name.position),
        );
    }
}
