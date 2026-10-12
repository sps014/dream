use super::{Case, Expectation, cases};
use dream::driver::compiler::Compiler;
use dream::driver::test::{TestHost, TestOptions, run_tests};
use dream::driver::toolchain::ToolchainConfig;
use dream::driver::wasm_opt::OptLevel;
use dream_mir::backend::Target;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;

fn release() -> bool {
    std::env::var_os("DREAM_CORPUS_RELEASE").is_some()
}

pub fn run_selected(stems: Option<&[&str]>, hosts: &[TestHost]) {
    run_selected_profile(stems, hosts, release());
}

pub fn run_selected_profile(stems: Option<&[&str]>, hosts: &[TestHost], release_build: bool) {
    let filter = std::env::var("DREAM_CORPUS_FILTER").ok();
    let concurrency = std::env::var_os("DREAM_CORPUS_CONCURRENCY").is_some();
    let selected: Vec<_> = cases()
        .filter(|case| {
            (!concurrency
                || [
                    "worker",
                    "task",
                    "async",
                    "thread",
                    "channel",
                    "mutex",
                    "parallel",
                    "shared",
                    "atomic",
                    "lock",
                    "semaphore",
                ]
                .iter()
                .any(|tag| case.stem.contains(tag)))
                && stems.is_none_or(|stems| stems.contains(&case.stem))
                && filter
                    .as_ref()
                    .is_none_or(|filter| case.stem.contains(filter))
        })
        .collect();
    assert!(!selected.is_empty(), "no corpus cases selected");
    let mut native_stdout = BTreeMap::new();
    let mut failures = Vec::new();
    for host in hosts {
        let mut projects: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        let mut isolated = Vec::new();
        for case in &selected {
            if *host == TestHost::Node && !case.node {
                println!("SKIP {} on node: native host required", case.stem);
                continue;
            }
            match case.expectation {
                Expectation::Project(suite) => projects
                    .entry(suite)
                    .or_default()
                    .push(case.stem.to_string()),
                _ => isolated.push(**case),
            }
        }
        let config = Arc::new(ToolchainConfig::default());
        let jobs: Vec<_> = projects.into_iter().collect();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(
                std::env::var("CORPUS_JOBS")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(4),
            )
            .build()
            .unwrap();
        let project_results: Vec<_> = pool.install(|| {
            jobs.par_iter()
                .map(|(suite, names)| {
                    let result = check(|| {
                        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/projects")
                            .join(suite);
                        let result = run_tests(
                            &config,
                            &path,
                            &TestOptions {
                                release: release_build,
                                debug_info: std::env::var_os("DREAM_CORPUS_DEBUG_INFO").is_some(),
                                selected_tests: names.clone(),
                                hosts: vec![*host],
                                ..Default::default()
                            },
                        )
                        .unwrap_or_else(|error| panic!("{error}"));
                        assert!(result.ok(), "{result:?}");
                        assert_eq!(result.reports[0].passed.len(), names.len());
                    });
                    (*suite, result)
                })
                .collect()
        });
        for (suite, result) in project_results {
            if let Err(error) = result {
                failures.push(format!("{suite} on {host:?}: {error}"));
            }
        }
        let outputs: Vec<_> = pool.install(|| {
            isolated
                .par_iter()
                .map(|case| {
                    (
                        case.stem,
                        check(|| run_fixture(*case, *host, release_build)),
                    )
                })
                .collect()
        });
        for (stem, output) in outputs {
            let output = match output {
                Ok(output) => output,
                Err(error) => {
                    failures.push(format!("{stem} on {host:?}: {error}"));
                    continue;
                }
            };
            if *host == TestHost::Native {
                native_stdout.insert(stem, output);
            } else if let Some(native) = native_stdout.get(stem)
                && native != &output
            {
                failures.push(format!(
                    "{stem} native/Node stdout differ: {native:?} != {output:?}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "corpus failures:\n{}",
        failures.join("\n")
    );
}

fn check<T>(run: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).map_err(|error| {
        error
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                error
                    .downcast_ref::<&str>()
                    .map(|message| message.to_string())
            })
            .unwrap_or_else(|| "test panicked".to_string())
    })
}

fn run_fixture(case: Case, host: TestHost, release_build: bool) -> Option<String> {
    let tmp = tempfile::Builder::new()
        .prefix(case.stem)
        .tempdir()
        .unwrap();
    let config = Arc::new(ToolchainConfig::default());
    let target = if host == TestHost::Native {
        Target::native()
    } else {
        Target::wasm32()
    };
    let output = tmp.path().join(if host == TestHost::Native {
        "guest.ll"
    } else {
        "guest.wat"
    });
    let mut compiler = Compiler::new_with_toolchain_config(target, config.clone())
        .with_render_diagnostics(false)
        .with_release(release_build)
        .with_debug_info(std::env::var_os("DREAM_CORPUS_DEBUG_INFO").is_some());
    if host == TestHost::Node {
        compiler = compiler.with_compile_targets(dream_abi::attributes::CompileTargets {
            native: false,
            node: true,
            web: false,
        });
    }
    let result = compiler.compile(
        &case.source().to_string_lossy().into_owned(),
        &output.to_string_lossy(),
    );
    if let Expectation::Error { native, node } = case.expectation {
        let want = if host == TestHost::Native {
            native
        } else {
            node
        };
        let error = result.expect_err("invalid corpus program compiled successfully");
        let text = error
            .diagnostic_text()
            .unwrap_or_else(|| panic!("{}: expected diagnostics, got {error:?}", case.stem));
        assert_fragments(text, want, case.stem);
        println!("PASS {} on {host:?} (diagnostics)", case.stem);
        return None;
    }
    result.unwrap_or_else(|error| panic!("{}: {error:?}", case.stem));
    let mut command = if host == TestHost::Native {
        let opt = if release_build {
            OptLevel::O3
        } else {
            OptLevel::O0
        };
        let bin = dream::execution::llvm::compile_llvm(
            &config,
            &output,
            dream::execution::llvm::NativeBuildOptions {
                target: dream_abi::target::TargetSpec::host(),
                opt_ll: None,
                opt_remarks: None,
                opt,
                profile: dream_abi::profile::CompileProfile::from_release(release_build),
                debug: false,
                pgo: &dream::execution::native::Pgo::Off,
                icon: None,
                relocatable: false,
                output_kind: dream::driver::output::OutputKind::Executable,
            },
        )
        .unwrap();
        dream::execution::native::native_command(
            &config,
            &bin,
            &output.to_string_lossy(),
            Some(Path::new(env!("CARGO_MANIFEST_DIR"))),
        )
        .unwrap()
    } else {
        let script = tmp.path().join("run.mjs");
        let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/dream.js");
        std::fs::write(&script, format!(
            "import {{ pathToFileURL }} from 'node:url';\nconst {{ run }} = await import(pathToFileURL({}).href);\nawait run({}, {{stdout: s => process.stdout.write(s), leakReport: true}});\n",
            serde_json::to_string(&runtime.to_string_lossy()).unwrap(),
            serde_json::to_string(&output.with_extension("wasm").to_string_lossy()).unwrap(),
        )).unwrap();
        let mut command = Command::new("node");
        command.arg(script).current_dir(env!("CARGO_MANIFEST_DIR"));
        command
    };
    if case.stem == "process_args_basic" {
        command.args(["alpha", "beta"]);
    }
    let stdin = (case.stem == "console_read_line").then_some(&b"hello-line\n"[..]);
    let result = capture(command, stdin);
    let stdout = String::from_utf8_lossy(&result.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&result.stderr).replace("\r\n", "\n");
    match case.expectation {
        Expectation::Trap(want) => {
            assert!(!result.status.success(), "{} should fail", case.stem);
            // `dream run` reports a signal-aborted guest as exit 1.
            let text = format!(
                "{stdout}\n{stderr}\nexit code {}",
                result.status.code().unwrap_or(1)
            );
            assert_fragments(&text, want, case.stem);
        }
        Expectation::Output(want) => {
            assert!(result.status.success(), "{}: {stderr}\n{stdout}", case.stem);
            assert_eq!(stdout.trim(), want.trim(), "{} on {host:?}", case.stem);
            for line in stderr.lines().chain(stdout.lines()) {
                if let Some(live) = line.split("[dream] leak check: live=").nth(1) {
                    assert!(
                        live.starts_with("0")
                            && !live.chars().nth(1).is_some_and(|c| c.is_ascii_digit()),
                        "{}: {line}",
                        case.stem
                    );
                }
            }
        }
        _ => unreachable!("shared tests run through dream test"),
    }
    println!("PASS {} on {host:?}", case.stem);
    Some(stdout)
}

fn assert_fragments(text: &str, want: &str, stem: &str) {
    for needle in want.lines().map(str::trim).filter(|line| !line.is_empty()) {
        assert!(text.contains(needle), "{stem}: missing {needle:?}\n{text}");
    }
}

fn capture(mut command: Command, stdin: Option<&[u8]>) -> Output {
    command
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().unwrap();
    if let Some(input) = stdin {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    let pid = child.id();
    let (sender, receiver) = std::sync::mpsc::channel();
    let waiter = std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    match receiver.recv_timeout(std::time::Duration::from_secs(180)) {
        Ok(result) => {
            waiter.join().unwrap();
            result.unwrap()
        }
        Err(error) => {
            #[cfg(unix)]
            {
                let _ = Command::new("kill")
                    .args(["-KILL", "--", &format!("-{pid}")])
                    .status();
            }
            #[cfg(windows)]
            {
                let _ = Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .status();
            }
            waiter.join().unwrap();
            panic!("corpus fixture timed out: {error}");
        }
    }
}

#[test]
#[ignore = "full corpus; scripts/test_corpus.py"]
fn full_corpus() {
    let hosts = std::env::var("DREAM_CORPUS_HOSTS").unwrap_or_else(|_| "native,node".into());
    let hosts: Vec<_> = hosts
        .split(',')
        .map(|host| match host {
            "native" => TestHost::Native,
            "node" => TestHost::Node,
            other => panic!("unknown host {other}"),
        })
        .collect();
    run_selected(None, &hosts);
}
