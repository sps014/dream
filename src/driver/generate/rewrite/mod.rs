//! In-arena rewrite of `SyntaxBlock` expressions into ordinary Dream expressions.

mod expression;
mod functions;
mod model;
mod parse;
mod spans;
mod statement;

pub use functions::{rewrite_expression, rewrite_function};
