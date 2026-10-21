//! The attribute contract types and generic lookups shared by every family.

use dream_syntax::nodes::AttributeNode;

/// The kind of declaration an attribute is attached to, coarse enough to express every current
/// placement rule (`@json` on a type, `@location` on a field, ...) without needing the
/// full declaration AST at validation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeTarget {
    Constructor,
    Destructor,
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
    /// A formal parameter.
    Parameter,
}

impl AttributeTarget {
    pub fn display_name(self) -> &'static str {
        match self {
            AttributeTarget::Constructor => "a constructor",
            AttributeTarget::Destructor => "a destructor",
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
/// constructor parameter type. Derived from the attribute declaration fields.
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
    /// A declaration reference, validated by its consuming generator framework.
    Path,
}

/// True when an attribute named `name` is present (even if its argument failed to parse).
pub fn has_named_attr(attributes: &[AttributeNode], name: &str) -> bool {
    attributes.iter().any(|a| is_compiler_attribute(a, name))
}

/// The `i`th string argument of `@name(...)`, or `None` when absent.
pub fn named_attr_string_arg<'a>(
    attributes: &'a [AttributeNode],
    name: &str,
    i: usize,
) -> Option<&'a str> {
    attributes
        .iter()
        .find(|a| is_compiler_attribute(a, name))?
        .args
        .get(i)?
        .as_string()
}

/// Only compiler-owned declarations activate compiler behavior.
pub fn compiler_attribute_module(name: &str) -> Option<&'static str> {
    Some(match name {
        "c" | "c_call" | "marshal" | "packed" | "cpp" | "cpp_name" => "system.interop",
        "test" | "timeout" => "system.testing",
        "generator" | "generator_only" | "incremental" | "on_attribute" | "on_call"
        | "syntax_block" | "uses_tool" => "system.codegen",
        "attribute" | "repeatable" | "export" | "intrinsic" | "llvm" | "inline" | "noinline"
        | "unsafe" | "borrowed_result" | "slot_walk" | "allow_cycle" | "consuming" | "owned"
        | "js" | "runtime" | "async_host" | "native" | "node" | "web" => "system.core",
        _ => return None,
    })
}

pub fn is_compiler_attribute(attr: &AttributeNode, name: &str) -> bool {
    compiler_attribute_module(name).is_some_and(|module| {
        attr.resolved_id
            .get()
            .is_some_and(|id| id == &compiler_attribute_identity(module, name))
    })
}

pub fn compiler_attribute_identity(module: &str, name: &str) -> String {
    use heck::ToUpperCamelCase;
    format!("{module}::{}Attribute", name.to_upper_camel_case())
}
