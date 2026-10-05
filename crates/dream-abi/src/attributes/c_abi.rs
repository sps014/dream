//! The `@c` family: C ABI bindings, calling conventions, marshaling and packing.

use super::*;

pub(super) const SPECS: &[AttributeSpec] = &[
    AttributeSpec {
        name: "c",
        targets: &[AttributeTarget::ExternFunction],
        args: ArgShape::Args {
            kinds: &[ArgKind::String, ArgKind::String],
            min: 0,
            max: 2,
        },
        repeatable: false,
        doc: "Binds an extern function to a C ABI library/symbol: `@c(\"lib\", \"symbol\")`. `@c(\"lib\")` uses the Dream name as the symbol; bare `@c` also binds to the declaring package's `native/` sources. Native and portable WASM builds are supported; runtime annotations restrict availability.",
    },
    AttributeSpec {
        name: "c_call",
        targets: &[AttributeTarget::ExternFunction],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "C calling convention for `@c` externs: `@c_call(\"cdecl\")` (the platform default) or `@c_call(\"stdcall\")` (Win32 APIs; identical to cdecl on every target but 32-bit x86 Windows).",
    },
    AttributeSpec {
        name: "marshal",
        targets: &[AttributeTarget::ExternFunction, AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "`@c` marshaling: on the extern, string encoding (`@marshal(\"lpstr\")` / `@marshal(\"lpwstr\")`); on a `NativeCallback` parameter, `@marshal(\"user_data_last\")` passes `user_data` after the callback's own arguments.",
    },
    AttributeSpec {
        name: "packed",
        targets: &[AttributeTarget::ValueStruct],
        args: ArgShape::None,
        repeatable: false,
        doc: "Packs a value struct with no padding for C ABI layout (`@packed`).",
    },
];

/// Extracts the `(lib, symbol)` pair from `@c("lib", "symbol")`, or `None` when absent.
pub fn c_import_target(attributes: &[AttributeNode]) -> Option<(String, String)> {
    let c = attributes.iter().find(|a| a.name.text == "c")?;
    let lib = c.args.first()?.as_string()?.to_string();
    let symbol = c.args.get(1)?.as_string()?.to_string();
    Some((lib, symbol))
}

/// True when the declaration carries `@c`.
pub fn has_c_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "c")
}

/// True when a value struct carries `@packed`.
pub fn has_packed_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "packed")
}

/// `@marshal("lpstr")` or `@marshal("lpwstr")`. `None` when absent (ANSI/`lpstr`).
pub fn c_marshal_charset(attributes: &[AttributeNode]) -> Option<&str> {
    let attr = attributes.iter().find(|a| a.name.text == "marshal")?;
    attr.args.first()?.as_string()
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
            "cdecl" => Some(CCallConv::Cdecl),
            "stdcall" => Some(CCallConv::Stdcall),
            _ => None,
        }
    }
}

/// The extern's `@c_call` convention (cdecl when absent or invalid; validation reports the latter).
pub fn c_call_conv(attributes: &[AttributeNode]) -> CCallConv {
    attributes
        .iter()
        .find(|a| a.name.text == "c_call")
        .and_then(|a| a.args.first()?.as_string())
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
    if extern_binding_conflict(attrs) {
        let pos = attrs
            .iter()
            .find(|a| matches!(a.name.text.as_str(), "c" | "js" | "runtime" | "intrinsic"))
            .map(|a| a.name.position);
        diagnostics.report_error(
            "an extern function cannot combine `@c`, `@js`, `@runtime`, or `@intrinsic`"
                .to_string(),
            pos,
        );
    }
    if !has_c_attr(attrs) {
        for name in ["marshal", "c_call"] {
            if let Some(attr) = attrs.iter().find(|a| a.name.text == name) {
                diagnostics.report_error(
                    format!("'@{name}' requires '@c' on the same extern (it only applies to C ABI imports)"),
                    Some(attr.name.position),
                );
            }
        }
    }
    if let Some(attr) = attrs.iter().find(|a| a.name.text == "c_call") {
        let conv = attr.args.first().and_then(|a| a.as_string());
        if conv.is_some_and(|c| CCallConv::parse(c).is_none()) {
            diagnostics.report_error(
                format!(
                    "unknown C calling convention '{}': use \"cdecl\" or \"stdcall\"",
                    conv.unwrap_or_default()
                ),
                Some(attr.name.position),
            );
        }
    }
}
