//! Host binding attributes (`@js`, `@runtime`, `@async_host`) and runtime gating
//! (`@native`, `@node`, `@web`).

use super::*;

pub(super) const SPECS: &[AttributeSpec] = &[
    AttributeSpec {
        name: "js",
        targets: &[AttributeTarget::ExternFunction],
        args: ArgShape::Args {
            kinds: &[ArgKind::String, ArgKind::String],
            min: 2,
            max: 2,
        },
        repeatable: false,
        doc: "Binds an extern function to a JavaScript host import: `@js(\"module\", \"export\")`. JS-only (user interop, `js` type, WASM `env`/libm). Dream runtime hosts that exist on native and WASM use `@runtime` instead.",
    },
    AttributeSpec {
        name: "runtime",
        targets: &[AttributeTarget::ExternFunction],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Binds an extern to the Dream runtime host on WASM and native: `@runtime(\"fileRead\")` imports `Dream.fileRead` and calls the C symbol `fileRead`.",
    },
    AttributeSpec {
        name: "async_host",
        targets: &[AttributeTarget::ExternFunction],
        args: ArgShape::None,
        repeatable: false,
        doc: "Deferred native host for an `extern async fun`: the `<host>Async` C symbol takes the future as its leading argument, performs the work off-thread, and completes it via the bound dream_complete_foreign. The run loop stays parked while the work is in flight.",
    },
    AttributeSpec {
        name: "native",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
            AttributeTarget::ExternFunction,
        ],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a function/method as available on the native (C) host. Combine with `@node`/`@web` to restrict; absent all three means every runtime.",
    },
    AttributeSpec {
        name: "node",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
            AttributeTarget::ExternFunction,
        ],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a function/method as available on the Node.js host. Combine with `@native`/`@web` to restrict; absent all three means every runtime.",
    },
    AttributeSpec {
        name: "web",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
            AttributeTarget::ExternFunction,
        ],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a function/method as available in the browser host. Combine with `@native`/`@node` to restrict; absent all three means every runtime.",
    },
];

/// Which runtimes a declaration is available on. Absent `@native`/`@node`/`@web` means all three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeSupport {
    pub native: bool,
    pub node: bool,
    pub web: bool,
}

impl RuntimeSupport {
    pub const ALL: Self = Self {
        native: true,
        node: true,
        web: true,
    };

    pub fn from_attributes(attributes: &[AttributeNode]) -> Self {
        let has_native = attributes.iter().any(|a| a.name.text == "native");
        let has_node = attributes.iter().any(|a| a.name.text == "node");
        let has_web = attributes.iter().any(|a| a.name.text == "web");
        if !has_native && !has_node && !has_web {
            return Self::ALL;
        }
        Self {
            native: has_native,
            node: has_node,
            web: has_web,
        }
    }

    pub fn display(&self) -> String {
        if self.native && self.node && self.web {
            return "all".to_string();
        }
        let mut parts = Vec::new();
        if self.native {
            parts.push("native");
        }
        if self.node {
            parts.push("node");
        }
        if self.web {
            parts.push("web");
        }
        parts.join(", ")
    }
}

/// Active compile-time runtime target(s) selected by the driver/CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompileTargets {
    pub native: bool,
    pub node: bool,
    pub web: bool,
}

impl CompileTargets {
    pub fn native_only() -> Self {
        Self {
            native: true,
            node: false,
            web: false,
        }
    }

    /// Every selected compile target must be listed in `support`.
    pub fn allows(&self, support: RuntimeSupport) -> bool {
        (!self.native || support.native)
            && (!self.node || support.node)
            && (!self.web || support.web)
    }

    pub fn display_list(&self) -> String {
        let mut parts = Vec::new();
        if self.native {
            parts.push("native");
        }
        if self.node {
            parts.push("node");
        }
        if self.web {
            parts.push("web");
        }
        parts.join(", ")
    }

    fn missing_targets(&self, support: RuntimeSupport) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.native && !support.native {
            missing.push("native");
        }
        if self.node && !support.node {
            missing.push("node");
        }
        if self.web && !support.web {
            missing.push("web");
        }
        missing
    }

    pub fn first_missing_target(&self, support: RuntimeSupport) -> Option<&'static str> {
        self.missing_targets(support).into_iter().next()
    }
}

/// Extracts the `(module, field)` pair from a `@js("module", "field")` attribute, or `None` if the
/// declaration carries no `@js` attribute. `validate_program_attributes` already guarantees that a
/// present `@js` has exactly two string arguments, so this never needs to fall back on a partial
/// match. Single source of truth for the extraction previously duplicated between `driver/abi.rs`
/// and `semantics::analyzer::hir_emit`.
pub fn js_import_target(attributes: &[AttributeNode]) -> Option<(String, String)> {
    let js = attributes.iter().find(|a| a.name.text == "js")?;
    let module = js.args.first()?.as_string()?.to_string();
    let field = js.args.get(1)?.as_string()?.to_string();
    Some((module, field))
}

/// Host field from `@runtime("fileRead")`, or `None` when the attribute is absent.
pub fn runtime_import_field(attributes: &[AttributeNode]) -> Option<String> {
    let attr = attributes.iter().find(|a| a.name.text == "runtime")?;
    attr.args.first()?.as_string().map(|s| s.to_string())
}

/// True when the declaration carries `@runtime`.
pub fn has_runtime_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "runtime")
}

/// True when more than one of `@c`, `@js`, `@runtime`, or `@intrinsic` is on the same extern.
pub fn extern_binding_conflict(attributes: &[AttributeNode]) -> bool {
    let n = u8::from(has_c_attr(attributes))
        + u8::from(js_import_target(attributes).is_some())
        + u8::from(has_runtime_attr(attributes))
        + u8::from(attributes.iter().any(|a| a.name.text == "intrinsic"));
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
