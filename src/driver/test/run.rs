//! Orchestrates `dream test`: locate the project, plan its runner, then build and run it on each
//! selected host and summarize.

use super::build::BuildRequest;
use super::project::{TestHost, TestProject};
use super::report::{HostReport, run_and_report};
use super::runner::RunnerPlan;
use crate::driver::ui::Ui;
use crate::driver::wasm_opt::OptLevel;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct TestOptions {
    pub release: bool,
    pub debug_info: bool,
    pub optimize: Option<OptLevel>,
    /// Keep only tests whose `<file>::<name>` label contains this; pruned before compiling.
    pub filter: Option<String>,
    /// Exact function names, for compiler integration suites; empty selects every matching test.
    pub selected_tests: Vec<String>,
    pub verbose: bool,
    /// Hosts to run on instead of the project's `[test].targets`.
    pub hosts: Vec<TestHost>,
}

#[derive(Debug)]
pub struct TestRunResult {
    pub project: String,
    pub reports: Vec<HostReport>,
}

impl TestRunResult {
    pub fn ok(&self) -> bool {
        self.reports.iter().all(HostReport::ok)
    }
}

/// Runs the test project at `path` (a test package, a directory of test files, or one file).
/// `Err` carries a setup or compile failure; failing tests come back in the result.
pub fn run_tests(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    path: &Path,
    opts: &TestOptions,
) -> Result<TestRunResult, String> {
    let project = TestProject::locate(path)?;
    if project.files.is_empty() {
        return Err(format!("no .dream test files under '{}'", path.display()));
    }
    let plan = RunnerPlan::discover(&project, opts.filter.as_deref(), &opts.selected_tests)?;
    if plan.tests.is_empty() && !plan.unparsed {
        return Err(format!(
            "'{}': no @test functions{}",
            path.display(),
            opts.filter
                .as_ref()
                .map(|f| format!(" matching filter '{f}'"))
                .unwrap_or_default()
        ));
    }
    let hosts = if opts.hosts.is_empty() {
        project.hosts.clone()
    } else {
        opts.hosts.clone()
    };
    let ui = Ui::new();
    let request = BuildRequest {
        config,
        project: &project,
        plan: &plan,
        release: opts.release,
        debug_info: opts.debug_info,
        optimize: opts.optimize,
    };
    let mut reports = Vec::new();
    for host in hosts {
        ui.step(
            "Testing",
            &format!(
                "{} on {} ({} test(s))",
                project.name,
                host.as_str(),
                plan.tests.len()
            ),
        );
        let lock_path = request.lock_path(host);
        std::fs::create_dir_all(lock_path.parent().unwrap()).map_err(|error| error.to_string())?;
        // Filtered runs share artifact names; keep a second build from replacing a running module.
        let _lock =
            crate::driver::output::acquire_lock(&lock_path).map_err(|error| error.to_string())?;
        let command = request.build(host)?;
        let report = run_and_report(command, host, &plan.tests)?;
        summarize(&ui, &report);
        reports.push(report);
    }
    Ok(TestRunResult {
        project: project.name,
        reports,
    })
}

fn summarize(ui: &Ui, report: &HostReport) {
    let host = report.host.as_str();
    for hint in &report.leak_hints {
        ui.note(&format!("LEAK {hint}"));
    }
    for (label, reason) in &report.failed {
        ui.error(&format!("{label} failed on {host}: {reason}"));
    }
    if !report.not_run.is_empty() {
        ui.error(&format!(
            "{} test(s) not run on {host}: {}",
            report.not_run.len(),
            report.not_run.join(", ")
        ));
    }
    if let Some(problem) = &report.problem {
        ui.error(&format!("{host}: {problem}"));
    }
    let counts = format!(
        "{} passed; {} failed; {} not run",
        report.passed.len(),
        report.failed.len(),
        report.not_run.len()
    );
    if report.ok() {
        ui.success(&format!("test result ({host}): ok. {counts}"));
    } else {
        ui.error(&format!("test result ({host}): FAILED. {counts}"));
    }
}
