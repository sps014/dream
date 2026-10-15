//! Declaration-driven registry and validator for the `@name(args)` attribute syntax.
//!
//! Attribute *parsing* (`crates/dream-syntax/src/parser/declarations/attributes.rs`) is,
//! and stays, fully generic: any `@identifier` or `@identifier(arg, ...)` parses on any
//! attribute-bearing declaration, with args classified as typed [`AttributeArg`] constants
//! (string/int/float/double/bool/enum path).
//!
//! Dream `@attribute` structs supply every public contract. This crate collects and validates
//! those contracts and provides typed readers for effects implemented by the compiler.

mod binding;
mod c_abi;
mod codegen;
mod collect;
mod cpp;
mod ownership;
mod spec;
mod testing;
#[cfg(test)]
mod tests;
mod user;
mod validate;

pub use binding::*;
pub use c_abi::*;
pub use codegen::*;
pub use cpp::*;
pub use ownership::*;
pub use spec::*;
pub use testing::*;
pub use user::*;
pub use validate::{validate_attributes, validate_program_attributes};

use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{AttributeArg, AttributeNode, Type};
