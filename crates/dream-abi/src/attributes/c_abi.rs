//! The `@c` family: C ABI bindings, calling conventions, marshaling and packing.

use super::*;

pub const C_ATTR: &str = "c";

/// Extracts the `(lib, symbol)` pair from `@c("lib", "symbol")`, or `None` when absent.
pub fn c_import_target(attributes: &[AttributeNode]) -> Option<(String, String)> {
    let c = attributes.iter().find(|a| is_compiler_attribute(a, C_ATTR))?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarshalKind {
    LpStr,
    LpWStr,
    UserDataLast,
}

pub fn marshal_kind(attributes: &[AttributeNode]) -> Option<MarshalKind> {
    let attr = attributes
        .iter()
        .find(|a| is_compiler_attribute(a, "marshal"))?;
    match enum_member(attr)? {
        "LpStr" => Some(MarshalKind::LpStr),
        "LpWStr" => Some(MarshalKind::LpWStr),
        "UserDataLast" => Some(MarshalKind::UserDataLast),
        _ => None,
    }
}

fn enum_member(attr: &AttributeNode) -> Option<&str> {
    let AttributeArg::Enum(path) = attr.args.first()? else {
        return None;
    };
    Some(&path.last()?.text)
}

/// A `@c_call` calling convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CCallConv {
    Cdecl,
    Stdcall,
}

impl CCallConv {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "Cdecl" => Some(CCallConv::Cdecl),
            "Stdcall" => Some(CCallConv::Stdcall),
            _ => None,
        }
    }
}

/// The extern's `@c_call` convention (cdecl when absent or invalid; validation reports the latter).
pub fn c_call_conv(attributes: &[AttributeNode]) -> CCallConv {
    attributes
        .iter()
        .find(|a| is_compiler_attribute(a, "c_call"))
        .and_then(enum_member)
        .and_then(CCallConv::parse)
        .unwrap_or(CCallConv::Cdecl)
}

/// Reports `@c`-family placement errors on one extern's attribute list:
/// - `@c` combined with `@js` / `@runtime` / `@intrinsic` (incompatible binding hosts),
/// - `@runtime` combined with `@js` / `@c` / `@intrinsic`,
/// - `@marshal(...)` without `@c` (only meaningful for the C ABI),
/// - `@c_call(...)` without `@c` (ditto),
/// - `@c_call` naming a convention other than `"cdecl"` / `"stdcall"`.
///
/// Call after generic attribute shape validation.
pub fn validate_c_extern_attrs(attrs: &[AttributeNode], diagnostics: &mut DiagnosticBag) {
    validate_marshal_context(attrs, false, diagnostics);
    if extern_binding_conflict(attrs) {
        let pos = attrs
            .iter()
            .find(|a| {
                [C_ATTR, "js", "runtime", crate::intrinsics::INTRINSIC_ATTR]
                    .iter()
                    .any(|name| is_compiler_attribute(a, name))
            })
            .map(|a| a.name.position);
        diagnostics.report_error(
            "an extern function cannot combine `@c`, `@js`, `@runtime`, or `@intrinsic`"
                .to_string(),
            pos,
        );
    }
    if !has_c_attr(attrs) {
        for name in ["marshal", "c_call"] {
            if let Some(attr) = attrs.iter().find(|a| is_compiler_attribute(a, name)) {
                diagnostics.report_error(
                    format!("'@{name}' requires '@c' on the same extern (it only applies to C ABI imports)"),
                    Some(attr.name.position),
                );
            }
        }
    }
}

pub fn validate_marshal_context(
    attrs: &[AttributeNode],
    parameter: bool,
    diagnostics: &mut DiagnosticBag,
) {
    if let Some(attr) = attrs.iter().find(|a| is_compiler_attribute(a, "marshal")) {
        let valid = match marshal_kind(attrs) {
            Some(MarshalKind::UserDataLast) => parameter,
            Some(MarshalKind::LpStr | MarshalKind::LpWStr) => !parameter,
            None => true,
        };
        if !valid {
            diagnostics.report_error("marshaling choice does not apply here: use LpStr/LpWStr on C externs or UserDataLast on NativeCallback parameters".to_string(), Some(attr.name.position));
        }
    }
}
