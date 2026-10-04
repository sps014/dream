//! Foreign-language packages consume target-built libraries; SDK linking remains explicit.

mod android;
mod bridge;
mod ios;
mod package;
#[cfg(test)]
mod tests;

#[cfg(test)]
use package::read_slices;
use package::{command, identifier, Slice};
pub use package::{run, Options};
