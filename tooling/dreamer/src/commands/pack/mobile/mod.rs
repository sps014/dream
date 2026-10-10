//! Foreign-language packages consume target-built libraries; SDK linking remains explicit.

mod android;
mod bridge;
mod ios;
mod package;
#[cfg(test)]
mod tests;

#[cfg(test)]
use package::read_slices;
pub use package::{Options, run};
use package::{Slice, command, identifier};
