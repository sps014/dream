//! The synthesized runner: one `main` that imports every project file and runs each selected
//! `@test` through `Test.run`, in file then declaration order.

use super::discovery::{DiscoveredTest, discover_tests_in_source};
use super::project::TestProject;
use std::fmt::Write as _;
use std::path::Path;

/// In-memory entry path of the runner, inside the project root so imports resolve from there.
pub const RUNNER_FILE: &str = "__dream_test_runner__.dream";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTest {
    /// `<file import path>::<function>`, the name `RUN` / `PASS` lines carry.
    pub label: String,
    pub function: String,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct RunnerPlan {
    pub imports: Vec<String>,
    pub tests: Vec<PlannedTest>,
    /// A file failed to parse; compiling the runner reports why.
    pub unparsed: bool,
}

impl RunnerPlan {
    /// Discovers every `@test` in the project and keeps those whose label contains `filter`.
    pub fn discover(project: &TestProject, filter: Option<&str>) -> Result<Self, String> {
        let mut imports = Vec::new();
        let mut tests = Vec::new();
        let mut unparsed = false;
        for file in &project.files {
            let source = std::fs::read_to_string(file)
                .map_err(|e| format!("read '{}': {e}", file.display()))?;
            let import = project.import_path(file)?;
            let Ok(found) = discover_tests_in_source(&file.to_string_lossy(), &source) else {
                // The runner compile renders the parse errors with source excerpts.
                unparsed = true;
                imports.push(import);
                continue;
            };
            if found.declares_main {
                return Err(format!(
                    "'{}': test files must not declare 'main'; use @test functions instead",
                    file.display()
                ));
            }
            for DiscoveredTest { name, timeout_ms } in found.tests {
                let label = format!("{import}::{name}");
                if filter.is_none_or(|f| label.contains(f)) {
                    tests.push(PlannedTest {
                        label,
                        function: name,
                        timeout_ms,
                    });
                }
            }
            imports.push(import);
        }
        Ok(Self {
            imports,
            tests,
            unparsed,
        })
    }

    pub fn source(&self) -> String {
        let mut out = String::from("import system;\nimport system.testing;\n");
        for import in &self.imports {
            let _ = writeln!(out, "import {import};");
        }
        out.push_str("\nfun main(): void {\n");
        for test in &self.tests {
            let label = test.label.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = match test.timeout_ms {
                Some(ms) => writeln!(
                    out,
                    "    Test.run_with_timeout(\"{label}\", {ms}, {});",
                    test.function
                ),
                None => writeln!(out, "    Test.run(\"{label}\", {});", test.function),
            };
        }
        out.push_str("}\n");
        out
    }

    pub fn entry_path(project_root: &Path) -> String {
        project_root.join(RUNNER_FILE).to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_runs_each_test_with_its_timeout() {
        let plan = RunnerPlan {
            imports: vec!["math".into(), "io.files".into()],
            tests: vec![
                PlannedTest {
                    label: "math::adds".into(),
                    function: "adds".into(),
                    timeout_ms: None,
                },
                PlannedTest {
                    label: "io.files::reads".into(),
                    function: "reads".into(),
                    timeout_ms: Some(500),
                },
            ],
            unparsed: false,
        };
        assert_eq!(
            plan.source(),
            "import system;\nimport system.testing;\nimport math;\nimport io.files;\n\n\
             fun main(): void {\n    Test.run(\"math::adds\", adds);\n    \
             Test.run_with_timeout(\"io.files::reads\", 500, reads);\n}\n"
        );
    }
}
