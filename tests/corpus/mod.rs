//! The probe corpus: shared Dream projects and process/compiler fixtures with inline assertions.

mod error;
mod isolated;
mod project;
pub(crate) mod run;
mod trap;

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub enum Expectation {
    Project(&'static str),
    Error {
        native: Option<&'static str>,
        node: Option<&'static str>,
    },
    Trap(&'static str),
    Output(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct Case {
    pub stem: &'static str,
    pub expectation: Expectation,
    pub node: bool,
    pub isolation: Option<&'static str>,
}

impl Case {
    const fn project(stem: &'static str, suite: &'static str, node: bool) -> Self {
        Self {
            stem,
            expectation: Expectation::Project(suite),
            node,
            isolation: None,
        }
    }
    const fn error(stem: &'static str, text: &'static str, node: bool) -> Self {
        Self {
            stem,
            expectation: Expectation::Error {
                native: Some(text),
                node: Some(text),
            },
            node,
            isolation: Some("invalid program"),
        }
    }
    // None verifies successful compilation without executing the counterpart target.
    const fn error_targets(
        stem: &'static str,
        native: Option<&'static str>,
        node: Option<&'static str>,
    ) -> Self {
        Self {
            stem,
            expectation: Expectation::Error { native, node },
            node: true,
            isolation: Some("invalid program"),
        }
    }
    const fn trap(stem: &'static str, text: &'static str, node: bool) -> Self {
        Self {
            stem,
            expectation: Expectation::Trap(text),
            node,
            isolation: Some("process-ending behavior"),
        }
    }
    const fn isolated(
        stem: &'static str,
        text: &'static str,
        node: bool,
        reason: &'static str,
    ) -> Self {
        Self {
            stem,
            expectation: Expectation::Output(text),
            node,
            isolation: Some(reason),
        }
    }

    pub fn source(self) -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        match self.expectation {
            Expectation::Project(suite) => root
                .join("tests/projects")
                .join(suite)
                .join(format!("{}.dream", self.stem)),
            _ => root
                .join("tests/cases")
                .join(format!("{}.dream", self.stem)),
        }
    }
}

pub fn cases() -> impl Iterator<Item = &'static Case> {
    project::CASES
        .iter()
        .chain(error::CASES)
        .chain(trap::CASES)
        .chain(isolated::CASES)
}

#[test]
fn every_case_has_one_source_and_assertions() {
    let mut names = std::collections::BTreeSet::new();
    let mut tests = std::collections::BTreeSet::new();
    for case in cases() {
        assert!(names.insert(case.stem), "duplicate {}", case.stem);
        if let Expectation::Project(suite) = case.expectation {
            for test in run::project_tests(case) {
                // The runner calls every selected test of a suite by its bare name.
                assert!(
                    tests.insert((suite, test.clone())),
                    "duplicate @test {test} in suite {suite}"
                );
            }
        }
        assert!(
            case.source().is_file(),
            "missing {}",
            case.source().display()
        );
        match case.expectation {
            Expectation::Project(_) => {
                let text = std::fs::read_to_string(case.source()).unwrap();
                assert!(text.contains("@test"), "{} has no test", case.stem);
                assert!(text.contains("Assert."), "{} has no assertions", case.stem);
            }
            _ => assert!(case.isolation.is_some()),
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for directory in [root.join("cases"), root.join("projects")] {
        let files = if directory.ends_with("cases") {
            std::fs::read_dir(&directory)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect()
        } else {
            super::common::corpus_files(&directory)
        };
        for file in files.into_iter().filter(|file| {
            file.extension()
                .is_some_and(|extension| extension == "dream")
        }) {
            assert!(
                cases().any(|case| case.source() == file),
                "unregistered {}",
                file.display()
            );
        }
    }
    for file in super::common::corpus_files(&root) {
        let name = file.file_name().unwrap().to_string_lossy();
        assert!(
            !name.contains(".expected"),
            "obsolete sidecar {}",
            file.display()
        );
    }
}
