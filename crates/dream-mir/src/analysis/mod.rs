//! Whole-module analyses shared by several passes.

pub(crate) mod escape;
pub(crate) mod object_life;
#[cfg(test)]
mod escape_tests;
#[cfg(test)]
mod object_life_tests;
