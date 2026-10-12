//! `dream test`: discover every `@test` in a test project, synthesize one runner `main` that calls
//! each through `Test.run`, compile it once per host (build cache on), run it and read its report.

mod build;
mod discovery;
mod project;
mod report;
mod run;
mod runner;

pub use discovery::{DiscoveredFile, DiscoveredTest, discover_tests_in_source};
pub use project::{TestHost, TestProject};
pub use report::HostReport;
pub use run::{TestOptions, TestRunResult, run_tests};
