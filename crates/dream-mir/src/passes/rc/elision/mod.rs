//! [`RcElision`]: cancel redundant Retain/Release pairs.

mod branches;
mod chains;
mod pipeline;
mod postdom;
#[cfg(test)]
mod tests;

pub use pipeline::RcElision;
