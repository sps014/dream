#![cfg(feature = "native")]

use dream::driver::test::{HostReport, TestHost, TestOptions, run_tests};
use dream::driver::toolchain::ToolchainConfig;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

const PRELUDE: &str = "import system;\nimport system.io;\nimport system.testing;\n\n";

fn project(targets: &str, files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let assets = if files.iter().any(|(name, _)| name.starts_with("data/")) {
        "assets = [\"data\"]\n"
    } else {
        ""
    };
    let manifest = format!(
        "[package]\nname = \"suite\"\nversion = \"0.1.0\"\ntype = \"test\"\n{assets}\n[test]\ntargets = [{targets}]\n"
    );
    fs::write(dir.path().join("dream.toml"), manifest).unwrap();
    for (name, body) in files {
        let path = dir.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text = if name.ends_with(".dream") {
            format!("{PRELUDE}{body}")
        } else {
            (*body).to_string()
        };
        fs::write(path, text).unwrap();
    }
    dir
}

fn run(dir: &Path, hosts: Vec<TestHost>) -> HostReport {
    let result = run_tests(
        &Arc::new(ToolchainConfig::default()),
        dir,
        &TestOptions {
            hosts,
            ..Default::default()
        },
    )
    .expect("test project should build");
    assert_eq!(result.reports.len(), 1);
    result.reports.into_iter().next().unwrap()
}

fn labels(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_string()).collect()
}

#[test]
fn passing_tests_across_files_build_one_runner() {
    let dir = project(
        "\"native\"",
        &[
            (
                "math.dream",
                "@test\nfun adds(): void { Assert.eq(1 + 1, 2); }\n",
            ),
            (
                "nested/text.dream",
                "@test\nfun joins(): void { Assert.eq(\"a\" + \"b\", \"ab\"); }\n",
            ),
            ("data/.keep", ""),
        ],
    );
    let report = run(dir.path(), Vec::new());
    assert!(report.ok(), "{report:?}");
    assert_eq!(report.passed, labels(&["math::adds", "nested.text::joins"]));
    assert!(dir.path().join("target/test/debug").is_dir());
}

#[test]
fn assert_failure_is_attributed_and_later_tests_are_not_run() {
    let dir = project(
        "\"native\"",
        &[(
            "cases.dream",
            "@test\nfun first(): void { Assert.eq(1, 1); }\n\
             @test\nfun second(): void { Assert.eq(1, 2); }\n\
             @test\nfun third(): void { Assert.eq(2, 2); }\n",
        )],
    );
    let report = run(dir.path(), Vec::new());
    assert!(!report.ok());
    assert_eq!(report.passed, labels(&["cases::first"]));
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, "cases::second");
    assert_eq!(report.not_run, labels(&["cases::third"]));
}

#[test]
fn trap_is_attributed_to_the_running_test() {
    let dir = project(
        "\"native\"",
        &[(
            "traps.dream",
            "@test\nfun out_of_bounds(): void {\n    let xs = [1, 2];\n    let i = xs.length + 3;\n    Assert.eq(xs[i], 0);\n}\n",
        )],
    );
    let report = run(dir.path(), Vec::new());
    assert!(!report.ok());
    assert!(report.passed.is_empty());
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert_eq!(report.failed[0].0, "traps::out_of_bounds");
}

#[test]
fn timeout_kills_the_runner_and_reports_the_rest_as_not_run() {
    let dir = project(
        "\"native\"",
        &[(
            "slow.dream",
            "@test\n@timeout(200)\nfun spins(): void {\n    let n = 0;\n    while (n >= 0) { n = (n + 1) % 1000; }\n}\n\
             @test\nfun after(): void { Assert.eq(1, 1); }\n",
        )],
    );
    let report = run(dir.path(), Vec::new());
    assert!(!report.ok());
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert_eq!(report.failed[0].0, "slow::spins");
    assert!(report.failed[0].1.contains("TIMEOUT"), "{report:?}");
    assert_eq!(report.not_run, labels(&["slow::after"]));
}

#[test]
fn leaked_objects_fail_the_exit_gate() {
    let dir = project(
        "\"native\"",
        &[(
            "cycle.dream",
            "@allow_cycle\nclass Node {\n    public next: Option<Node>;\n    public constructor() { this.next = Option.None; }\n}\n\n\
             @test\nfun leaks_a_cycle(): void {\n    let node = Node();\n    node.next = Option.Some(node);\n}\n",
        )],
    );
    let report = run(dir.path(), Vec::new());
    assert_eq!(report.passed, labels(&["cycle::leaks_a_cycle"]));
    assert_eq!(report.leak_hints.len(), 1, "{report:?}");
    assert!(!report.ok());
    assert!(report.problem.is_some());
}

#[test]
fn assets_are_readable_from_the_runner_directory() {
    let dir = project(
        "\"native\"",
        &[
            ("data/words.txt", "alpha\nbeta\n"),
            (
                "assets.dream",
                "@test\nfun reads_asset(): void {\n    let stream = File.open(\"data/words.txt\").unwrap();\n    Assert.eq(stream.read_all().unwrap(), \"alpha\\nbeta\\n\");\n    stream.close().unwrap();\n}\n",
            ),
        ],
    );
    let report = run(dir.path(), Vec::new());
    assert!(report.ok(), "{report:?}");
    assert!(
        dir.path()
            .join("target/test/debug/data/words.txt")
            .is_file()
    );
}

#[test]
fn node_host_runs_the_wasm_runner_with_assets() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("skipping: node is not installed");
        return;
    }
    let dir = project(
        "\"node\"",
        &[
            ("data/flag.txt", "on"),
            (
                "node_cases.dream",
                "@test\nfun sees_asset(): void { Assert.is_true(File.exists(\"data/flag.txt\")); }\n\
                 @test\nfun counts(): void { Assert.eq([1, 2, 3].length, 3); }\n",
            ),
        ],
    );
    let report = run(dir.path(), Vec::new());
    assert_eq!(report.host, TestHost::Node);
    assert!(report.ok(), "{report:?}");
    assert_eq!(
        report.passed,
        labels(&["node_cases::sees_asset", "node_cases::counts"])
    );
}

#[test]
fn async_tests_finish_before_the_next_test_on_both_hosts() {
    for host in [TestHost::Native, TestHost::Node] {
        let dir = project(
            "\"native\", \"node\"",
            &[(
                "ordered.dream",
                "import system.testing;\nlet stage = 0;\n\
                @test async fun first(): void { Time.sleep(5).await; stage = 1; }\n\
                @test fun second(): void { Assert.eq(stage, 1); stage = 2; }\n\
                @test async fun third(): void { Time.sleep(5).await; Assert.eq(stage, 2); }\n",
            )],
        );
        let report = run(dir.path(), vec![host]);
        assert!(report.ok(), "{report:?}");
        assert_eq!(
            report.passed,
            labels(&["ordered::first", "ordered::second", "ordered::third"])
        );
    }
}

#[test]
fn async_timeout_is_attributed_on_both_hosts() {
    for host in [TestHost::Native, TestHost::Node] {
        let dir = project(
            "\"native\", \"node\"",
            &[(
                "slow.dream",
                "import system.testing;\n@test @timeout(100) async fun slow(): void { Time.sleep(1).await; let n = 0; while n >= 0 { n = (n + 1) % 1000; } }\n\
                @test fun after(): void { Assert.is_true(true); }\n",
            )],
        );
        let report = run(dir.path(), vec![host]);
        assert_eq!(report.failed.len(), 1, "{report:?}");
        assert_eq!(report.failed[0].0, "slow::slow");
        assert!(report.failed[0].1.contains("TIMEOUT"), "{report:?}");
        assert_eq!(report.not_run, labels(&["slow::after"]));
    }
}

#[test]
fn concurrent_filtered_runs_keep_their_own_runner() {
    for host in [TestHost::Native, TestHost::Node] {
        let dir = project(
            "\"native\", \"node\"",
            &[(
                "concurrent.dream",
                "import system.testing;\n\
            @test async fun passes(): void { Time.delay(100).await; Assert.eq(1, 1); }\n\
            @test fun fails(): void { Assert.eq(1, 2); }\n",
            )],
        );
        std::thread::scope(|scope| {
            let handles: Vec<_> = ["passes", "fails"]
                .into_iter()
                .map(|name| {
                    let path = dir.path();
                    scope.spawn(move || {
                        let result = run_tests(
                            &Arc::new(ToolchainConfig::default()),
                            path,
                            &TestOptions {
                                selected_tests: vec![name.into()],
                                hosts: vec![host],
                                ..Default::default()
                            },
                        )
                        .unwrap();
                        assert_eq!(result.ok(), name == "passes", "{result:?}");
                        let report = &result.reports[0];
                        assert_eq!(report.passed.len() + report.failed.len(), 1);
                        if name == "passes" {
                            assert_eq!(report.passed, labels(&["concurrent::passes"]));
                        } else {
                            assert_eq!(report.failed[0].0, "concurrent::fails");
                        }
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap();
            }
        });
    }
}
