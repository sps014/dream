//! Central registry and validator for the `@name(args)` attribute syntax.
//!
//! Attribute *parsing* (`crates/dream-syntax/src/parser/declarations/attributes.rs`) is,
//! and stays, fully generic: any `@identifier` or `@identifier(arg, ...)` parses on any
//! attribute-bearing declaration, with args classified as typed [`AttributeArg`] constants
//! (string/int/float/double/bool/enum path). Historically every consumer
//! (`@intrinsic`, `@json`, `@property_name`, `@override`, `@js`, `@allow_cycle`) then hand-rolled
//! its own `attributes.iter().any(|a| a.name.text == "...")` check with no shared validation, so an
//! unknown attribute name (a typo like `@josn`) or a misapplied one (`@json` on a function) was
//! silently accepted and simply had no effect.
//!
//! This module is the single place that knows the full set of attribute names the compiler
//! recognizes, which kinds of declarations each may appear on, and what shape its arguments must
//! take. [`validate_program_attributes`] walks every attribute-bearing declaration once (called
//! from the driver, before semantic analysis) and reports unknown names, disallowed placements,
//! wrong argument counts, and (for non-repeatable attributes) duplicates. Attribute-specific
//! *meaning* (e.g. `@override` may only target `to_string`/`hash_code`, `@operator` must resolve
//! to a known operator symbol) is layered on top by each feature's own code, which can then assume
//! the generic shape/placement contract already holds.

use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::function::FunctionNode;
use dream_syntax::nodes::interface_node::InterfaceDeclarationNode;
use dream_syntax::nodes::program::{EnumDeclarationNode, ExtendNode};
use dream_syntax::nodes::struct_node::{StructDeclarationNode, StructFieldNode};
use dream_syntax::nodes::types::is_special_member_name;
use dream_syntax::nodes::{AttributeArg, AttributeNode, Type};
use std::collections::BTreeMap;
use std::rc::Rc;

/// The kind of declaration an attribute is attached to, coarse enough to express every current
/// placement rule (`@json` on a type, `@override` on an instance method, ...) without needing the
/// full declaration AST at validation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeTarget {
    /// A top-level, non-`extern` function.
    Function,
    /// A non-`static`, non-`extern` method on a `class`/`struct`/`extend` block.
    Method,
    /// A `static`, non-`extern` method on a `class`/`struct`/`extend` block.
    StaticMethod,
    /// Any function or method (top-level, instance, or static) declared `extern`.
    ExternFunction,
    /// A field on a `class`/`struct`/enum-variant payload.
    Field,
    /// A reference-type (`class`) declaration.
    Struct,
    /// A value-type (`struct`) declaration.
    ValueStruct,
    /// A plain C-style `enum` (no variant carries a payload).
    PlainEnum,
    /// A discriminated union (an `enum` where at least one variant carries a payload).
    Union,
    /// An `interface` declaration.
    Interface,
    /// A method signature inside an `interface`.
    InterfaceMethod,
    /// A file-level `module` declaration.
    Module,
    /// A formal parameter (`@readonly a: GpuBuffer<T>`).
    Parameter,
}

impl AttributeTarget {
    pub fn display_name(self) -> &'static str {
        match self {
            AttributeTarget::Function => "a top-level function",
            AttributeTarget::Method => "an instance method",
            AttributeTarget::StaticMethod => "a static method",
            AttributeTarget::ExternFunction => "an extern function/method",
            AttributeTarget::Field => "a field",
            AttributeTarget::Struct => "a class",
            AttributeTarget::ValueStruct => "a struct",
            AttributeTarget::PlainEnum => "a plain enum",
            AttributeTarget::Union => "a discriminated union",
            AttributeTarget::Interface => "an interface",
            AttributeTarget::InterfaceMethod => "an interface method",
            AttributeTarget::Module => "a module declaration",
            AttributeTarget::Parameter => "a function parameter",
        }
    }
}

/// Kind of a single attribute argument — the attribute registry's analogue of a C# attribute
/// constructor parameter type. Declared once on the [`AttributeSpec`]; instances don't invent shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgKind {
    /// A string literal (`"module"`, `"field"`, …).
    String,
    /// An integer literal (`8`, `64`, …).
    Int,
    /// An unsuffixed or `f`-suffixed float literal (`3.14`, `1.0f`).
    Float,
    /// A `d`-suffixed double literal (`3.14d`).
    Double,
    /// A boolean literal (`true` / `false`).
    Bool,
    /// A dotted enum-member path (`HttpMethod.Get`).
    Enum,
}

/// The expected shape of an attribute's argument list — the closed-world "constructor signature"
/// for builtin attributes. User-defined `@attribute` functions supply their schema from the
/// function parameters instead.
#[derive(Debug, Clone, Copy)]
pub enum ArgShape {
    /// `@name` with no `(...)` at all, or empty parens.
    None,
    /// `@name(...)` with between `min` and `max` (inclusive) arguments.
    /// Argument `i` must match `kinds[i.min(kinds.len() - 1)]` (so a single-kind slice covers
    /// variadic same-typed args like `@compute(8, 8)`).
    Args {
        kinds: &'static [ArgKind],
        min: usize,
        max: usize,
    },
}

/// One attribute's full contract: its name, the declaration kinds it may appear on, its argument
/// shape, whether it may be repeated on the same declaration, and a short doc string for IDE
/// hover/completion.
pub struct AttributeSpec {
    pub name: &'static str,
    pub targets: &'static [AttributeTarget],
    pub args: ArgShape,
    pub repeatable: bool,
    /// Markdown-friendly one-liner (or short paragraph) shown in LSP hover/completion docs.
    pub doc: &'static str,
}

impl ArgShape {
    /// Human-readable parameter labels for signature help (empty for [`ArgShape::None`]).
    pub fn param_labels(self) -> Vec<&'static str> {
        match self {
            ArgShape::None => Vec::new(),
            ArgShape::Args { kinds, min, max } => {
                let n = max.max(min).max(kinds.len());
                (0..n)
                    .map(|i| match kinds[i.min(kinds.len() - 1)] {
                        ArgKind::String => "string",
                        ArgKind::Int => "int",
                        ArgKind::Float => "float",
                        ArgKind::Double => "double",
                        ArgKind::Bool => "bool",
                        ArgKind::Enum => "Enum.Member",
                    })
                    .collect()
            }
        }
    }

    /// Signature label like `@intrinsic(string)` or `@js(string, string)`.
    pub fn signature_label(self, name: &str) -> String {
        match self {
            ArgShape::None => format!("@{name}"),
            ArgShape::Args { .. } => {
                let params = self.param_labels().join(", ");
                format!("@{name}({params})")
            }
        }
    }
}

/// Looks up a builtin attribute by name.
pub fn find_spec(name: &str) -> Option<&'static AttributeSpec> {
    ATTRIBUTES.iter().find(|s| s.name == name)
}

/// Every attribute name the compiler recognizes. Adding a new attribute means adding one entry
/// here; [`validate_program_attributes`] then enforces its placement/shape everywhere, and the
/// feature module only needs to implement attribute-specific *meaning* on top.
pub const ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        name: "intrinsic",
        targets: &[AttributeTarget::ExternFunction],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Marks an extern function/method as a compiler intrinsic. The string argument is the intrinsic key (e.g. `\"print\"`).",
    },
    AttributeSpec {
        name: "json",
        targets: &[
            AttributeTarget::Struct,
            AttributeTarget::ValueStruct,
            AttributeTarget::Union,
        ],
        args: ArgShape::None,
        repeatable: false,
        doc: "Enables JSON serialize/deserialize derive for a class, struct, or discriminated union.",
    },
    AttributeSpec {
        name: "property_name",
        targets: &[AttributeTarget::Field],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Overrides the JSON property name for a field (used with `@json`).",
    },
    AttributeSpec {
        name: "json_ignore",
        targets: &[AttributeTarget::Field],
        args: ArgShape::None,
        repeatable: false,
        doc: "Excludes a field from JSON serialize/deserialize (used with `@json`).",
    },
    AttributeSpec {
        name: "inline",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::None,
        repeatable: false,
        doc: "Raises the inliner's size budget for this function/method. A compiler hint, not a guarantee.",
    },
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
        name: "allow_cycle",
        targets: &[AttributeTarget::Struct],
        args: ArgShape::None,
        repeatable: false,
        doc: "Allows a class to participate in a reference cycle (ARC will not free it automatically).",
    },
    AttributeSpec {
        name: "unsafe",
        // Gates manual-memory-management operations (raw `Pointer<T>`): calling an `@unsafe`
        // function/method is only permitted from another `@unsafe` function/method — checked at
        // every call site, not just here at the declaration (see
        // `FunctionTableInfo::is_unsafe`/`Analyzer::check_unsafe_call`).
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
            AttributeTarget::ExternFunction,
        ],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a function/method as unsafe. Calls are only allowed from other `@unsafe` contexts.",
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
    // Source-generator framework (`system.codegen` / `driver/generate`).
    AttributeSpec {
        name: "generator",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a function as a source generator entry point (`system.codegen`).",
    },
    AttributeSpec {
        name: "test",
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a top-level `fun name(): void` as a unit test discovered by `dream test` / `dreamer test`.",
    },
    AttributeSpec {
        name: "syntax_block",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: true,
        doc: "Associates a generator with a syntax-block kind (string name).",
    },
    // User-defined attribute: `@attribute` on a bare top-level function; the function name is the
    // attribute name (exact casing), and its parameters are the `@name(...)` arg schema.
    AttributeSpec {
        name: "attribute",
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Declares a user-defined attribute. The function name becomes the `@name` attribute.",
    },
    // WebGPU compute kernels: body is emitted as WGSL, not WASM. Optional 1–3 int args are the
    // workgroup size (X[, Y[, Z]]); bare `@compute` defaults to (64, 1, 1).
    AttributeSpec {
        name: "compute",
        targets: &[AttributeTarget::Function],
        args: ArgShape::Args {
            kinds: &[ArgKind::Int],
            min: 0,
            max: 3,
        },
        repeatable: false,
        doc: "Marks a function as a WebGPU compute kernel. Optional ints are workgroup size X[, Y[, Z]] (default 64, 1, 1).",
    },
    // Storage-buffer access mode for `@compute` params: WGSL `var<storage, read>` instead of
    // `read_write`. Only meaningful on `GpuBuffer<T>` kernel parameters.
    AttributeSpec {
        name: "readonly",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::None,
        repeatable: false,
        doc: "On a `@compute` `GpuBuffer` parameter: storage access is read-only (WGSL `read`).",
    },
    // Cubemap texture parameter attribute: WGSL `texture_cube<f32>`.
    AttributeSpec {
        name: "cube",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::None,
        repeatable: false,
        doc: "On a `GpuTexture` parameter: binds a cubemap texture (WGSL `texture_cube<f32>`).",
    },
    // WebGPU vertex stage: body emitted as WGSL, not WASM.
    AttributeSpec {
        name: "vertex",
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a function as a WebGPU vertex shader. Body is emitted as WGSL, not WASM.",
    },
    // WebGPU fragment stage: body emitted as WGSL, not WASM.
    AttributeSpec {
        name: "fragment",
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a function as a WebGPU fragment shader. Body is emitted as WGSL, not WASM.",
    },
    // GPU helper: callable from `@compute`/`@vertex`/`@fragment` and emitted as a WGSL `fn`.
    AttributeSpec {
        name: "gpu",
        targets: &[AttributeTarget::Function],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks a helper callable from GPU shaders; body is also emitted as WGSL when referenced.",
    },
    // Optional vertex/varying location remap; default is declaration order.
    AttributeSpec {
        name: "location",
        targets: &[AttributeTarget::Field],
        args: ArgShape::Args {
            kinds: &[ArgKind::Int],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Optional WGSL @location(N) override on a vertex-attribute or varying field (default: declaration order).",
    },
    // WGSL `@builtin(name)` on a struct field (e.g. `@builtin("position")`, `@builtin("frag_depth")`).
    AttributeSpec {
        name: "builtin",
        targets: &[AttributeTarget::Field],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Marks a shader I/O field as a WGSL builtin (e.g. `\"position\"`, `\"frag_depth\"`). A field named `position: GpuVec4` is still accepted as sugar for `@builtin(\"position\")`.",
    },
    // WGSL `@interpolate(mode)` on a varying field.
    AttributeSpec {
        name: "interpolate",
        targets: &[AttributeTarget::Field],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "WGSL interpolation qualifier on a varying (`\"perspective\"`, `\"linear\"`, or `\"flat\"`).",
    },
    // Explicit bind-group index override (default group 0).
    AttributeSpec {
        name: "group",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::Int],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Optional WGSL `@group(N)` override on a shader resource parameter (default: 0).",
    },
    // Explicit binding index override within a group.
    AttributeSpec {
        name: "binding",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::Int],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Optional WGSL `@binding(N)` override on a shader resource parameter (default: auto-assigned).",
    },
    AttributeSpec {
        name: "c",
        targets: &[AttributeTarget::ExternFunction],
        args: ArgShape::Args {
            kinds: &[ArgKind::String, ArgKind::String],
            min: 2,
            max: 2,
        },
        repeatable: false,
        doc: "Binds an extern function to a native C ABI library/symbol: `@c(\"lib\", \"symbol\")`. Native-only (`@native` is optional; `@node`/`@web` are rejected).",
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
        doc: "C calling convention for `@c` externs: `@c_call(\"cdecl\")` or `@c_call(\"stdcall\")`.",
    },
    AttributeSpec {
        name: "marshal",
        targets: &[AttributeTarget::ExternFunction],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "String marshaling for `@c` externs: `@marshal(\"lpstr\")` or `@marshal(\"lpwstr\")`.",
    },
    AttributeSpec {
        name: "packed",
        targets: &[AttributeTarget::ValueStruct],
        args: ArgShape::None,
        repeatable: false,
        doc: "Packs a value struct with no padding for C ABI layout (`@packed`).",
    },
    AttributeSpec {
        name: "get",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP GET route (`system.webapi`): `@get(\"/items/{id}\")`.",
    },
    AttributeSpec {
        name: "post",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP POST route (`system.webapi`): `@post(\"/items\")`.",
    },
    AttributeSpec {
        name: "put",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP PUT route (`system.webapi`).",
    },
    AttributeSpec {
        name: "patch",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP PATCH route (`system.webapi`).",
    },
    AttributeSpec {
        name: "delete",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP DELETE route (`system.webapi`).",
    },
    AttributeSpec {
        name: "head",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP HEAD route (`system.webapi`).",
    },
    AttributeSpec {
        name: "options",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP OPTIONS route (`system.webapi`).",
    },
    AttributeSpec {
        name: "middleware",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::Int],
            min: 0,
            max: 1,
        },
        repeatable: false,
        doc: "Registers an onion middleware (`system.webapi`). Optional `order` int; lower runs first.",
    },
    AttributeSpec {
        name: "use",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::Enum],
            min: 1,
            max: 1,
        },
        repeatable: true,
        doc: "Attaches named middleware to one route: `@use(require_auth)`.",
    },
    AttributeSpec {
        name: "dep",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::Enum],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Injects a dependency function (FastAPI `Depends`): `@dep(current_user) user: User`.",
    },
    AttributeSpec {
        name: "query",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 0,
            max: 1,
        },
        repeatable: false,
        doc: "Binds a query parameter; optional name override `@query(\"q\")`.",
    },
    AttributeSpec {
        name: "header",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 0,
            max: 1,
        },
        repeatable: false,
        doc: "Binds a request header; optional name `@header(\"Authorization\")`.",
    },
    AttributeSpec {
        name: "cookie",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 0,
            max: 1,
        },
        repeatable: false,
        doc: "Binds a cookie; optional name `@cookie(\"session\")`.",
    },
    AttributeSpec {
        name: "body",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::None,
        repeatable: false,
        doc: "Binds the JSON (or raw) request body (`system.webapi`).",
    },
    AttributeSpec {
        name: "path",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 0,
            max: 1,
        },
        repeatable: false,
        doc: "Binds a path parameter when the name differs from `{segment}`: `@path(\"id\")`.",
    },
];

/// True when a parameter carries `@readonly` (compute storage → WGSL `read`).
pub fn has_readonly_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "readonly")
}

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
        if has_c_attr(attributes) {
            return Self {
                native: true,
                node: false,
                web: false,
            };
        }
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

fn arg_matches_kind(arg: &AttributeArg, kind: ArgKind) -> bool {
    match (kind, arg) {
        (ArgKind::String, AttributeArg::String(_)) => true,
        (ArgKind::Int, AttributeArg::Int(_)) => true,
        (ArgKind::Float, AttributeArg::Float(_)) => true,
        // Allow unsuffixed float literal when a double param is expected (same as expression
        // expected-type retargeting for numeric literals).
        (ArgKind::Double, AttributeArg::Double(_) | AttributeArg::Float(_)) => true,
        (ArgKind::Bool, AttributeArg::Bool(_)) => true,
        (ArgKind::Enum, AttributeArg::Enum(_)) => true,
        _ => false,
    }
}

fn kind_name(kind: ArgKind) -> &'static str {
    match kind {
        ArgKind::String => "a string literal",
        ArgKind::Int => "an integer literal",
        ArgKind::Float => "a float literal",
        ArgKind::Double => "a double literal",
        ArgKind::Bool => "a boolean literal",
        ArgKind::Enum => "an enum member path",
    }
}

fn type_to_arg_kind(ty: &Type) -> Option<ArgKind> {
    match ty {
        Type::String(_) => Some(ArgKind::String),
        Type::Integer(_) | Type::Byte(_) | Type::Long(_) | Type::UInt(_) | Type::ULong(_) => {
            Some(ArgKind::Int)
        }
        Type::Float(_) => Some(ArgKind::Float),
        Type::Double(_) => Some(ArgKind::Double),
        Type::Boolean(_) => Some(ArgKind::Bool),
        // Bare named types in params are treated as enum (e.g. `HttpMethod`).
        Type::Struct(_, None) | Type::Generic(_) => Some(ArgKind::Enum),
        _ => None,
    }
}

/// Top-level functions marked `@attribute`: name (exact casing) → parameter kinds.
fn collect_user_attributes(
    functions: &[FunctionNode<'_>],
    diagnostics: &mut DiagnosticBag,
) -> BTreeMap<String, Vec<ArgKind>> {
    let mut out = BTreeMap::new();
    for f in functions {
        if !f.attributes.iter().any(|a| a.name.text == "attribute") {
            continue;
        }
        diagnostics.file_path = file_path_string(&f.file_path);
        let mut kinds = Vec::new();
        let mut ok = true;
        for p in &f.parameters {
            match type_to_arg_kind(&p.type_) {
                Some(k) => kinds.push(k),
                None => {
                    diagnostics.report_error(
                        format!(
                            "attribute function '{}': parameter '{}' has a type that cannot be used as an attribute argument",
                            f.name.text, p.name.text
                        ),
                        Some(p.name.position),
                    );
                    ok = false;
                }
            }
        }
        if ok {
            if out.contains_key(&f.name.text) {
                diagnostics.report_error(
                    format!("duplicate attribute function '{}'", f.name.text),
                    Some(f.name.position),
                );
            } else {
                out.insert(f.name.text.clone(), kinds);
            }
        }
    }
    out
}

fn validate_arg_list(
    attr_name: &str,
    attr: &AttributeNode,
    kinds: &[ArgKind],
    min: usize,
    max: usize,
    diagnostics: &mut DiagnosticBag,
) {
    if attr.args.len() < min || attr.args.len() > max {
        let expected = if min == max {
            format!("{}", min)
        } else {
            format!("{}-{}", min, max)
        };
        diagnostics.report_error(
            format!(
                "'@{}' expects {} argument(s), got {}",
                attr_name,
                expected,
                attr.args.len()
            ),
            Some(attr.name.position),
        );
    }
    if kinds.is_empty() {
        return;
    }
    for (i, arg) in attr.args.iter().enumerate() {
        let kind = kinds[i.min(kinds.len() - 1)];
        if !arg_matches_kind(arg, kind) {
            diagnostics.report_error(
                format!(
                    "'@{}' argument {} must be {}, got '{}'",
                    attr_name,
                    i + 1,
                    kind_name(kind),
                    arg.display()
                ),
                Some(arg.position()),
            );
        }
    }
}

/// Validates one declaration's attribute list against `target`: every attribute must be a known
/// builtin name or a user `@attribute` function, allowed on `target`, carry the right argument shape,
/// and (unless `repeatable`) appear at most once.
pub fn validate_attributes(
    attrs: &[AttributeNode],
    target: AttributeTarget,
    diagnostics: &mut DiagnosticBag,
) {
    validate_attributes_with(attrs, target, &BTreeMap::new(), diagnostics);
}

fn validate_attributes_with(
    attrs: &[AttributeNode],
    target: AttributeTarget,
    user_attrs: &BTreeMap<String, Vec<ArgKind>>,
    diagnostics: &mut DiagnosticBag,
) {
    let mut seen: Vec<&str> = Vec::new();
    for attr in attrs {
        let name = attr.name.text.as_str();
        if let Some(spec) = find_spec(name) {
            if !spec.targets.contains(&target) {
                diagnostics.report_error(
                    format!("'@{}' cannot be applied to {}", name, target.display_name()),
                    Some(attr.name.position),
                );
            }
            match spec.args {
                ArgShape::None => {
                    if !attr.args.is_empty() {
                        diagnostics.report_error(
                            format!("'@{}' does not take any arguments", name),
                            Some(attr.name.position),
                        );
                    }
                }
                ArgShape::Args { kinds, min, max } => {
                    validate_arg_list(name, attr, kinds, min, max, diagnostics);
                }
            }
            if !spec.repeatable && seen.contains(&name) {
                diagnostics.report_error(
                    format!("duplicate '@{}' attribute", name),
                    Some(attr.name.position),
                );
            }
            seen.push(name);
            continue;
        }

        if let Some(kinds) = user_attrs.get(name) {
            let n = kinds.len();
            validate_arg_list(name, attr, kinds, n, n, diagnostics);
            if seen.contains(&name) {
                diagnostics.report_error(
                    format!("duplicate '@{}' attribute", name),
                    Some(attr.name.position),
                );
            }
            seen.push(name);
            continue;
        }

        diagnostics.report_error(
            format!("unknown attribute '@{}'", name),
            Some(attr.name.position),
        );
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

/// `@c_call("cdecl")` or `@c_call("stdcall")`. `None` when absent (platform default).
pub fn c_call_convention(attributes: &[AttributeNode]) -> Option<&str> {
    let attr = attributes.iter().find(|a| a.name.text == "c_call")?;
    attr.args.first()?.as_string()
}

/// `@marshal("lpstr")` or `@marshal("lpwstr")`. `None` when absent (ANSI/`lpstr`).
pub fn c_marshal_charset(attributes: &[AttributeNode]) -> Option<&str> {
    let attr = attributes.iter().find(|a| a.name.text == "marshal")?;
    attr.args.first()?.as_string()
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

/// Reports `@c`-family placement errors on one extern's attribute list:
/// - `@c` combined with `@js` / `@runtime` / `@intrinsic` (incompatible binding hosts),
/// - `@runtime` combined with `@js` / `@c` / `@intrinsic`,
/// - `@c` combined with `@node` or `@web` (`@c` is native-only; `@native` is allowed),
/// - `@marshal(...)` without `@c` (only meaningful for the C ABI),
/// - `@c_call(...)` without `@c` (ditto).
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
    if has_c_attr(attrs) {
        for name in ["node", "web"] {
            if let Some(attr) = attrs.iter().find(|a| a.name.text == name) {
                diagnostics.report_error(
                    format!("'@c' is native-only and cannot be combined with '@{name}'"),
                    Some(attr.name.position),
                );
            }
        }
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
}

/// True when the declaration carries `@compute`.
pub fn has_compute_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "compute")
}

/// True when the declaration carries `@test`.
pub fn has_test_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "test")
}

/// True when the declaration carries `@generator`.
pub fn has_generator_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "generator")
}

/// True when the declaration carries `@vertex`.
pub fn has_vertex_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "vertex")
}

/// True when the declaration carries `@fragment`.
pub fn has_fragment_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "fragment")
}

/// True when the declaration carries `@gpu` (shader-callable helper).
pub fn has_gpu_helper_attr(attributes: &[AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == "gpu")
}

/// True when an extern declaration carries `@async_host`: on native, its host function accepts
/// the future as its leading argument and completes it from another thread (returning 1 for
/// deferred work), instead of blocking inside the poll. wasm32 bridges are always deferred.
pub fn has_async_host_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, "async_host")
}

/// True when the declaration carries `@inline` (raised inliner size budget).
pub fn has_inline_attr(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, "inline")
}

/// True when the declaration is any GPU shader stage (`@compute` / `@vertex` / `@fragment`).
pub fn is_gpu_shader_attr(attributes: &[AttributeNode]) -> bool {
    has_compute_attr(attributes) || has_vertex_attr(attributes) || has_fragment_attr(attributes)
}

fn parse_named_u32(attributes: &[AttributeNode], name: &str) -> Option<u32> {
    attributes
        .iter()
        .find(|a| a.name.text == name)
        .and_then(|a| a.args.first())
        .and_then(|t| t.as_int_text())
        .and_then(dream_syntax::number::parse_u32_literal)
}

/// True when an attribute named `name` is present (even if its argument failed to parse).
pub fn has_named_attr(attributes: &[AttributeNode], name: &str) -> bool {
    attributes.iter().any(|a| a.name.text == name)
}

/// Optional `@location(N)` on a struct field. `None` when absent or not a valid `u32`.
pub fn field_location_override(attributes: &[AttributeNode]) -> Option<u32> {
    parse_named_u32(attributes, "location")
}

/// Optional `@builtin("name")` on a struct field. `None` when absent or malformed.
pub fn field_builtin_name(attributes: &[AttributeNode]) -> Option<String> {
    let attr = attributes.iter().find(|a| a.name.text == "builtin")?;
    attr.args
        .first()
        .and_then(|t| t.as_string())
        .map(|s| s.to_string())
}

/// Optional `@interpolate("mode")` on a varying field. `None` when absent or malformed.
pub fn field_interpolate_mode(attributes: &[AttributeNode]) -> Option<String> {
    let attr = attributes.iter().find(|a| a.name.text == "interpolate")?;
    attr.args
        .first()
        .and_then(|t| t.as_string())
        .map(|s| s.to_string())
}

/// Optional `@group(N)` on a shader parameter. `None` when absent or not a valid `u32`.
pub fn param_group_override(attributes: &[AttributeNode]) -> Option<u32> {
    parse_named_u32(attributes, "group")
}

/// Optional `@binding(N)` on a shader parameter. `None` when absent or not a valid `u32`.
pub fn param_binding_override(attributes: &[AttributeNode]) -> Option<u32> {
    parse_named_u32(attributes, "binding")
}

/// True when a field is the clip-space position builtin (`@builtin("position")` or name `position`).
pub fn field_is_position_builtin(name: &str, attributes: &[AttributeNode]) -> bool {
    if let Some(b) = field_builtin_name(attributes) {
        return b == "position";
    }
    name == "position"
}

/// Workgroup size from `@compute` / `@compute(x[, y[, z]])`. Bare `@compute` is `(64, 1, 1)`.
/// Present arguments must parse as `u32` (decimal/hex/bin/oct); they are never silently defaulted.
pub fn compute_workgroup_size(attributes: &[AttributeNode]) -> Result<(u32, u32, u32), String> {
    let Some(attr) = attributes.iter().find(|a| a.name.text == "compute") else {
        return Ok((64, 1, 1));
    };
    if attr.args.is_empty() {
        return Ok((64, 1, 1));
    }
    let parse = |i: usize| -> Result<u32, String> {
        let arg = &attr.args[i];
        arg.as_int_text()
            .and_then(dream_syntax::number::parse_u32_literal)
            .ok_or_else(|| {
                format!(
                    "@compute workgroup size '{}' is not a valid u32",
                    arg.display()
                )
            })
    };
    match attr.args.len() {
        1 => Ok((parse(0)?, 1, 1)),
        2 => Ok((parse(0)?, parse(1)?, 1)),
        _ => Ok((parse(0)?, parse(1)?, parse(2)?)),
    }
}

fn file_path_string(file_path: &Option<Rc<str>>) -> Option<String> {
    file_path.as_ref().map(|p| p.to_string())
}

/// The target kind for a function/method declaration, derived from its own modifiers. `None` for
/// constructors/destructors, which cannot carry attributes today.
fn function_target(f: &FunctionNode<'_>) -> Option<AttributeTarget> {
    if is_special_member_name(&f.name.text) {
        return None;
    }
    Some(if f.is_extern {
        AttributeTarget::ExternFunction
    } else if f.is_static {
        AttributeTarget::StaticMethod
    } else {
        AttributeTarget::Method
    })
}

fn validate_function_list(
    functions: &[FunctionNode<'_>],
    top_level: bool,
    user_attrs: &BTreeMap<String, Vec<ArgKind>>,
    diagnostics: &mut DiagnosticBag,
) {
    for f in functions {
        diagnostics.file_path = file_path_string(&f.file_path);
        if let Some(target) = function_target(f) {
            let target = if matches!(target, AttributeTarget::Method) && top_level {
                AttributeTarget::Function
            } else {
                target
            };
            validate_attributes_with(&f.attributes, target, user_attrs, diagnostics);
            if matches!(target, AttributeTarget::ExternFunction) {
                validate_c_extern_attrs(&f.attributes, diagnostics);
            }
        }
        for p in &f.parameters {
            validate_attributes_with(
                &p.attributes,
                AttributeTarget::Parameter,
                user_attrs,
                diagnostics,
            );
        }
    }
}

fn validate_fields(
    fields: &[StructFieldNode],
    user_attrs: &BTreeMap<String, Vec<ArgKind>>,
    diagnostics: &mut DiagnosticBag,
) {
    for field in fields {
        validate_attributes_with(
            &field.attributes,
            AttributeTarget::Field,
            user_attrs,
            diagnostics,
        );
    }
}

/// Walks every attribute-bearing declaration in the (fully merged, pre-derive) program once,
/// reporting unknown/misapplied/malformed attributes. Run from the driver right after source
/// loading and prelude merge, before `@json` derivation and semantic analysis, so both of those
/// later stages can assume every attribute they see already has valid shape and placement.
/// Synthesized declarations (`file_path: None` for structs/enums/functions, or
/// `is_synthesized` for `extend` blocks) are compiler-generated and always skipped.
pub fn validate_program_attributes(
    structs: &[StructDeclarationNode<'_>],
    interfaces: &[InterfaceDeclarationNode<'_>],
    functions: &[FunctionNode<'_>],
    enums: &[EnumDeclarationNode<'_>],
    extends: &[ExtendNode<'_>],
    diagnostics: &mut DiagnosticBag,
) {
    let user_attrs = collect_user_attributes(functions, diagnostics);

    for s in structs {
        if s.file_path.is_none() {
            continue;
        }
        diagnostics.file_path = file_path_string(&s.file_path);
        let target = if s.is_value {
            AttributeTarget::ValueStruct
        } else {
            AttributeTarget::Struct
        };
        validate_attributes_with(&s.attributes, target, &user_attrs, diagnostics);
        validate_fields(&s.fields, &user_attrs, diagnostics);
        validate_function_list(&s.methods, false, &user_attrs, diagnostics);
    }

    for i in interfaces {
        if i.file_path.is_none() {
            continue;
        }
        diagnostics.file_path = file_path_string(&i.file_path);
        validate_attributes_with(
            &i.attributes,
            AttributeTarget::Interface,
            &user_attrs,
            diagnostics,
        );
        for m in &i.methods {
            validate_attributes_with(
                &m.attributes,
                AttributeTarget::InterfaceMethod,
                &user_attrs,
                diagnostics,
            );
        }
    }

    validate_function_list(functions, true, &user_attrs, diagnostics);

    for e in enums {
        if e.file_path.is_none() {
            continue;
        }
        diagnostics.file_path = file_path_string(&e.file_path);
        let target = if e.is_data_enum() {
            AttributeTarget::Union
        } else {
            AttributeTarget::PlainEnum
        };
        validate_attributes_with(&e.attributes, target, &user_attrs, diagnostics);
        for v in &e.variants {
            validate_fields(&v.fields, &user_attrs, diagnostics);
        }
        validate_function_list(&e.methods, false, &user_attrs, diagnostics);
    }

    for ext in extends {
        if ext.is_synthesized {
            continue;
        }
        diagnostics.file_path = file_path_string(&ext.file_path);
        validate_function_list(&ext.methods, false, &user_attrs, diagnostics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dream_syntax::nodes::AttributeArg;
    use dream_syntax::token::syntax_token::SyntaxToken;
    use dream_syntax::token::token_kind::TokenKind;
    use dream_text::line_text::LineText;
    use dream_text::text_span::TextSpan;

    fn ident(text: &str) -> SyntaxToken {
        let span = TextSpan::new((0, 0), &LineText::new(String::new()));
        SyntaxToken::new(TokenKind::IdentifierToken, span, text.to_string())
    }

    fn str_arg(text: &str) -> AttributeArg {
        let span = TextSpan::new((0, 0), &LineText::new(String::new()));
        AttributeArg::String(SyntaxToken::new(
            TokenKind::StringToken,
            span,
            text.to_string(),
        ))
    }

    fn attr(name: &str, args: &[&str]) -> AttributeNode {
        AttributeNode {
            name: ident(name),
            args: args.iter().map(|a| str_arg(a)).collect(),
        }
    }

    #[test]
    fn unknown_attribute_is_reported() {
        let mut diagnostics = DiagnosticBag::new(None);
        validate_attributes(
            &[attr("bogus", &[])],
            AttributeTarget::Method,
            &mut diagnostics,
        );
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn misapplied_attribute_is_reported() {
        let mut diagnostics = DiagnosticBag::new(None);
        validate_attributes(
            &[attr("json", &[])],
            AttributeTarget::Function,
            &mut diagnostics,
        );
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn wrong_arg_count_is_reported() {
        let mut diagnostics = DiagnosticBag::new(None);
        validate_attributes(
            &[attr("intrinsic", &[])],
            AttributeTarget::ExternFunction,
            &mut diagnostics,
        );
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn duplicate_non_repeatable_attribute_is_reported() {
        let mut diagnostics = DiagnosticBag::new(None);
        validate_attributes(
            &[attr("inline", &[]), attr("inline", &[])],
            AttributeTarget::Method,
            &mut diagnostics,
        );
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn inline_is_accepted_on_methods_and_rejected_on_externs() {
        let mut diagnostics = DiagnosticBag::new(None);
        validate_attributes(
            &[attr("inline", &[])],
            AttributeTarget::Method,
            &mut diagnostics,
        );
        validate_attributes(
            &[attr("inline", &[])],
            AttributeTarget::StaticMethod,
            &mut diagnostics,
        );
        validate_attributes(
            &[attr("inline", &[])],
            AttributeTarget::Function,
            &mut diagnostics,
        );
        assert!(!diagnostics.has_errors());
        validate_attributes(
            &[attr("inline", &[])],
            AttributeTarget::ExternFunction,
            &mut diagnostics,
        );
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn well_formed_attribute_is_accepted() {
        let mut diagnostics = DiagnosticBag::new(None);
        validate_attributes(
            &[attr("intrinsic", &["\"print\""])],
            AttributeTarget::ExternFunction,
            &mut diagnostics,
        );
        assert!(!diagnostics.has_errors());
    }

    #[test]
    fn c_import_target_extracts_lib_and_symbol() {
        let attrs = &[attr("c", &["\"sqlite3\"", "\"sqlite3_open\""])];
        assert_eq!(
            c_import_target(attrs),
            Some(("sqlite3".to_string(), "sqlite3_open".to_string()))
        );
        assert!(has_c_attr(attrs));
        assert_eq!(
            extern_import_target(attrs, "fallback"),
            ("c/sqlite3".to_string(), "sqlite3_open".to_string())
        );
    }

    #[test]
    fn extern_binding_conflict_detected() {
        let attrs = &[
            attr("c", &["\"sqlite3\"", "\"sqlite3_open\""]),
            attr("js", &["\"Dream\"", "\"open\""]),
        ];
        assert!(extern_binding_conflict(attrs));
        let mut diagnostics = DiagnosticBag::new(None);
        validate_c_extern_attrs(attrs, &mut diagnostics);
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn runtime_import_targets_dream_module() {
        let attrs = &[attr("runtime", &["\"fileRead\""])];
        assert_eq!(runtime_import_field(attrs).as_deref(), Some("fileRead"));
        assert_eq!(
            extern_import_target(attrs, "fallback"),
            ("Dream".to_string(), "fileRead".to_string())
        );
        assert!(!extern_binding_conflict(attrs));
    }

    #[test]
    fn runtime_conflicts_with_js() {
        let attrs = &[
            attr("runtime", &["\"fileRead\""]),
            attr("js", &["\"Dream\"", "\"fileRead\""]),
        ];
        assert!(extern_binding_conflict(attrs));
        let mut diagnostics = DiagnosticBag::new(None);
        validate_c_extern_attrs(attrs, &mut diagnostics);
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn c_attr_implies_native_only_runtime() {
        let attrs = &[attr("c", &["\"m\"", "\"f\""])];
        let support = RuntimeSupport::from_attributes(attrs);
        assert!(support.native);
        assert!(!support.node);
        assert!(!support.web);
    }

    #[test]
    fn c_with_native_is_accepted() {
        let attrs = &[attr("c", &["\"m\"", "\"f\""]), attr("native", &[])];
        let mut diagnostics = DiagnosticBag::new(None);
        validate_c_extern_attrs(attrs, &mut diagnostics);
        assert!(!diagnostics.has_errors());
        let support = RuntimeSupport::from_attributes(attrs);
        assert!(support.native);
        assert!(!support.node);
        assert!(!support.web);
    }

    #[test]
    fn c_with_web_or_node_is_rejected() {
        for host in ["web", "node"] {
            let attrs = &[attr("c", &["\"m\"", "\"f\""]), attr(host, &[])];
            let mut diagnostics = DiagnosticBag::new(None);
            validate_c_extern_attrs(attrs, &mut diagnostics);
            if !diagnostics.has_errors() {
                panic!("expected '@c' + '@{}' to be rejected", host);
            }
        }
    }

    #[test]
    fn marshal_without_c_is_rejected() {
        // `@marshal` only affects `@c` externs — attaching it to a plain (or `@js`) extern is a
        // no-op and probably a bug, so the validator flags it.
        let attrs = &[attr("marshal", &["\"lpwstr\""])];
        let mut diagnostics = DiagnosticBag::new(None);
        validate_c_extern_attrs(attrs, &mut diagnostics);
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn c_call_without_c_is_rejected() {
        let attrs = &[attr("c_call", &["\"stdcall\""])];
        let mut diagnostics = DiagnosticBag::new(None);
        validate_c_extern_attrs(attrs, &mut diagnostics);
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn c_with_marshal_and_c_call_is_accepted() {
        let attrs = &[
            attr("c", &["\"user32\"", "\"MessageBoxW\""]),
            attr("marshal", &["\"lpwstr\""]),
            attr("c_call", &["\"stdcall\""]),
        ];
        let mut diagnostics = DiagnosticBag::new(None);
        validate_c_extern_attrs(attrs, &mut diagnostics);
        assert!(!diagnostics.has_errors());
    }

    fn int_arg(text: &str) -> AttributeArg {
        let span = TextSpan::new((0, 0), &LineText::new(String::new()));
        AttributeArg::Int(SyntaxToken::new(
            TokenKind::NumberToken,
            span,
            text.to_string(),
        ))
    }

    fn attr_ints(name: &str, args: &[&str]) -> AttributeNode {
        AttributeNode {
            name: ident(name),
            args: args.iter().map(|a| int_arg(a)).collect(),
        }
    }

    #[test]
    fn compute_workgroup_parses_hex_and_bin() {
        assert_eq!(
            compute_workgroup_size(&[attr_ints("compute", &["0x40"])]).unwrap(),
            (64, 1, 1)
        );
        assert_eq!(
            compute_workgroup_size(&[attr_ints("compute", &["0b1000", "0o10"])]).unwrap(),
            (8, 8, 1)
        );
    }

    #[test]
    fn compute_workgroup_rejects_unparseable_size() {
        assert!(compute_workgroup_size(&[attr_ints("compute", &["-1"])]).is_err());
    }

    #[test]
    fn location_override_parses_hex() {
        assert_eq!(
            field_location_override(&[attr_ints("location", &["0x10"])]),
            Some(16)
        );
    }
}
