//! Central registry and validator for the `@name(args)` attribute syntax.
//!
//! Attribute *parsing* (`crates/dream-syntax/src/parser/declarations/attributes.rs`) is,
//! and stays, fully generic: any `@identifier` or `@identifier(arg, ...)` parses on any
//! attribute-bearing declaration, with args classified as typed [`AttributeArg`] constants
//! (string/int/float/double/bool/enum path).
//!
//! This module is the single place that knows the full set of attribute names the compiler
//! recognizes, which kinds of declarations each may appear on, and what shape its arguments must
//! take. Each family file owns its specs and the typed readers for them. [`validate_program_attributes`]
//! walks every attribute-bearing declaration once (called from the driver, before semantic
//! analysis) and reports unknown names, disallowed placements, wrong argument counts, and (for
//! non-repeatable attributes) duplicates. Attribute-specific *meaning* is layered on top by each
//! feature's own code, which can then assume the generic shape/placement contract already holds.

mod binding;
mod c_abi;
mod codegen;
mod cpp;
mod gpu;
mod http;
mod ownership;
mod serialization;
mod spec;
mod testing;
#[cfg(test)]
mod tests;
mod validate;

pub use binding::*;
pub use c_abi::*;
pub use codegen::*;
pub use cpp::*;
pub use gpu::*;
pub use ownership::*;
pub use spec::*;
pub use testing::*;
pub use validate::{validate_attributes, validate_program_attributes};

use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{AttributeArg, AttributeNode, Type};

const FAMILIES: &[&[AttributeSpec]] = &[
    ownership::SPECS,
    codegen::SPECS,
    serialization::SPECS,
    binding::SPECS,
    testing::SPECS,
    gpu::SPECS,
    c_abi::SPECS,
    cpp::SPECS,
    http::SPECS,
];

/// Every attribute the compiler recognizes, in registry order.
pub fn all_specs() -> impl Iterator<Item = &'static AttributeSpec> {
    FAMILIES.iter().flat_map(|family| family.iter())
}

/// Looks up a builtin attribute by name.
pub fn find_spec(name: &str) -> Option<&'static AttributeSpec> {
    all_specs().find(|s| s.name == name)
}
