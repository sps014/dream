//! The attribute contract types and generic lookups shared by every family.

use dream_syntax::nodes::AttributeNode;

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

pub(super) fn parse_named_u32(attributes: &[AttributeNode], name: &str) -> Option<u32> {
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

/// The `i`th string argument of `@name(...)`, or `None` when absent.
pub fn named_attr_string_arg<'a>(
    attributes: &'a [AttributeNode],
    name: &str,
    i: usize,
) -> Option<&'a str> {
    attributes
        .iter()
        .find(|a| a.name.text == name)?
        .args
        .get(i)?
        .as_string()
}
