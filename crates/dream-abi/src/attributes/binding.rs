//! Host binding attributes (`@js`, `@runtime`, `@async_host`, `@main_loop`).

use super::*;

/// Extracts the `(module, field)` pair from a `@js("module", "field")` attribute, or `None` if the
/// declaration carries no `@js` attribute. `validate_program_attributes` already guarantees that a
/// present `@js` has exactly two string arguments, so this never needs to fall back on a partial
/// match. Single source of truth for the extraction previously duplicated between `driver/abi.rs`
/// and `semantics::analyzer::hir_emit`.
pub fn js_import_target(attributes: &[AttributeNode]) -> Option<(String, String)> {
    let js = attributes.iter().find(|a| is_compiler_attribute(a, "js"))?;
    let module = js.args.first()?.as_string()?.to_string();
    let field = js.args.get(1)?.as_string()?.to_string();
    Some((module, field))
}

/// Host field from `@runtime("fileRead")`, or `None` when the attribute is absent.
pub fn runtime_import_field(attributes: &[AttributeNode]) -> Option<String> {
    let attr = attributes
        .iter()
        .find(|a| is_compiler_attribute(a, "runtime"))?;
    attr.args.first()?.as_string().map(|s| s.to_string())
}

/// Native capability named by `@runtime("field", "net")`, or `None` for core host services.
pub fn runtime_host_capability(
    attributes: &[AttributeNode],
) -> Option<crate::host_capability::HostCapability> {
    let attr = attributes
        .iter()
        .find(|a| is_compiler_attribute(a, "runtime"))?;
    crate::host_capability::HostCapability::from_name(attr.args.get(1)?.as_string()?)
}

/// True when `@runtime(name, host, traits...)` lists `RuntimeTrait.<member>`.
fn runtime_has_trait(attributes: &[AttributeNode], member: &str) -> bool {
    attributes
        .iter()
        .filter(|a| is_compiler_attribute(a, "runtime"))
        .flat_map(|a| a.args.iter().skip(2))
        .any(|arg| matches!(arg, AttributeArg::Enum(path) if path.last().is_some_and(|p| p.text == member)))
}

/// True for a host service the WASI component adapter implements (`RuntimeTrait.Component`).
pub fn runtime_is_component(attributes: &[AttributeNode]) -> bool {
    runtime_has_trait(attributes, "Component")
}

/// True for a host service that reads host state only and never touches Dream storage or calls
/// back (`RuntimeTrait.Quiet`).
pub fn runtime_is_quiet(attributes: &[AttributeNode]) -> bool {
    runtime_has_trait(attributes, "Quiet")
}

/// `@requires_member(parameter, member, message?)` uses, in declaration order.
pub fn member_requirements(attributes: &[AttributeNode]) -> Vec<(String, String, Option<String>)> {
    attributes
        .iter()
        .filter(|a| is_compiler_attribute(a, "requires_member"))
        .filter_map(|a| {
            Some((
                a.args.first()?.as_string()?.to_string(),
                a.args.get(1)?.as_string()?.to_string(),
                a.args
                    .get(2)
                    .and_then(|arg| arg.as_string())
                    .map(str::to_string),
            ))
        })
        .collect()
}

/// True when the declaration carries `@runtime`.
pub fn has_runtime_attr(attributes: &[AttributeNode]) -> bool {
    attributes
        .iter()
        .any(|a| is_compiler_attribute(a, "runtime"))
}

pub const MAIN_LOOP_ATTR: &str = "main_loop";

/// True when the extern carries `@main_loop` (it owns the main thread on Android and iOS).
pub fn has_main_loop_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, MAIN_LOOP_ATTR)
}

/// True when more than one of `@c`, `@js`, `@runtime`, `@intrinsic`, or `@llvm` is on the same
/// extern.
pub fn extern_binding_conflict(attributes: &[AttributeNode]) -> bool {
    let n = u8::from(has_c_attr(attributes))
        + u8::from(js_import_target(attributes).is_some())
        + u8::from(has_runtime_attr(attributes))
        + u8::from(has_named_attr(
            attributes,
            crate::intrinsics::INTRINSIC_ATTR,
        ))
        + u8::from(has_named_attr(attributes, crate::intrinsics::LLVM_ATTR));
    n > 1
}

/// WASM import `(module, field)`: `@c` → `("c/<lib>", symbol)`; `@runtime` → `("Dream", name)`;
/// else `@js`; else `("env", default_field)`.
pub fn extern_import_target(attributes: &[AttributeNode], default_field: &str) -> (String, String) {
    if let Some((lib, symbol)) = c_import_target(attributes) {
        return (format!("c/{lib}"), symbol);
    }
    if let Some(field) = runtime_import_field(attributes) {
        return (crate::js_abi::HOST_MODULE.to_string(), field);
    }
    if let Some((module, field)) = js_import_target(attributes) {
        return (module, field);
    }
    ("env".to_string(), default_field.to_string())
}

/// True when an extern declaration carries `@async_host`: on native, its host function accepts
/// the future as its leading argument and completes it from another thread (returning 1 for
/// deferred work), instead of blocking inside the poll. wasm32 bridges are always deferred.
pub fn has_async_host_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, "async_host")
}
