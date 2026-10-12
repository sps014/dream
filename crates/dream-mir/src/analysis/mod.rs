//! Whole-module analyses shared by several passes.

pub(crate) mod effects;
#[cfg(test)]
mod effects_tests;
pub(crate) mod escape;
#[cfg(test)]
mod escape_tests;
pub(crate) mod function_key;
pub(crate) mod object_life;
#[cfg(test)]
mod object_life_tests;
