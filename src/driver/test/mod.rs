//! `@test` discovery and `dream test` runner: synthesize a `main` that calls each test via
//! `Test.run`, compile as a bin, and execute.

mod discovery;
mod run;

pub use discovery::{DiscoveredTest, discover_tests_in_source};
pub use run::{TestOptions, TestRunResult, run_tests};
