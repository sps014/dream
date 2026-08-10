//! `@test` discovery and `dream test` runner: synthesize a `main` that calls each test via
//! `Test.run`, compile as a bin, and execute.

mod discovery;
mod run;

pub use discovery::{discover_tests_in_source, DiscoveredTest};
pub use run::{run_tests, TestOptions, TestRunResult};
