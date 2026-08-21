//! Orchestrate `dream test`: discover `@test` fns, write a runner under `target/test/`, compile+run.

use super::discovery::{discover_tests_in_source, DiscoveredTest};
use crate::driver::compiler::{Compiler, Target};
use crate::driver::ui::Ui;
use crate::driver::wasm_opt::OptLevel;
use dream_sema::analyzer::CrateType;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::debug;

#[derive(Debug, Clone, Default)]
pub struct TestOptions {
    pub release: bool,
    pub optimize: Option<OptLevel>,
    pub filter: Option<String>,
    pub verbose: bool,
}

#[derive(Debug, Default)]
pub struct TestRunResult {
    pub files_run: usize,
    pub tests_run: usize,
    pub files_failed: usize,
}

/// Run tests for `path` (a `.dream` file or a directory of them). Returns `Ok` when every suite
/// exited successfully; `Err` with a short summary otherwise.
pub fn run_tests(path: &Path, opts: &TestOptions) -> Result<TestRunResult, String> {
    let files = collect_test_files(path)?;
    if files.is_empty() {
        return Err(format!("no .dream test files under '{}'", path.display()));
    }

    let ui = Ui::new();
    let mut result = TestRunResult::default();
    for file in &files {
        ui.step("Testing", &file.display().to_string());
        let start = Instant::now();
        match run_one_file(file, opts) {
            Ok(n) => {
                result.files_run += 1;
                result.tests_run += n;
            }
            Err(e) => {
                result.files_run += 1;
                result.files_failed += 1;
                ui.error(&e);
            }
        }
        debug!("{} finished in {:.2}s", file.display(), start.elapsed().as_secs_f64());
    }

    if result.files_failed > 0 {
        return Err(format!(
            "{} of {} test file(s) failed ({} test(s) started)",
            result.files_failed, result.files_run, result.tests_run
        ));
    }
    ui.success(&format!(
        "test result: ok. {} passed in {} file(s)",
        result.tests_run, result.files_run
    ));
    Ok(result)
}

fn collect_test_files(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.is_file() {
        if path.extension().and_then(|e| e.to_str()) != Some("dream") {
            return Err(format!("expected a .dream file, got '{}'", path.display()));
        }
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        return Err(format!("path not found: '{}'", path.display()));
    }
    let mut out = Vec::new();
    collect_dream_files_rec(path, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_dream_files_rec(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("read {}: {}", dir.display(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read {}: {}", dir.display(), e))?;
        let p = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "target" || name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            collect_dream_files_rec(&p, out)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some("dream")
            && !name.ends_with(".runner.dream")
        {
            out.push(p);
        }
    }
    Ok(())
}

fn run_one_file(path: &Path, opts: &TestOptions) -> Result<usize, String> {
    let path_str = path.to_string_lossy().into_owned();
    let source =
        fs::read_to_string(path).map_err(|e| format!("read '{}': {}", path.display(), e))?;

    let mut tests = discover_tests_in_source(&path_str, &source)?;
    if let Some(filter) = &opts.filter {
        tests.retain(|t| t.name.contains(filter.as_str()));
    }
    if tests.is_empty() {
        return Err(format!(
            "'{}': no @test functions{}",
            path.display(),
            opts.filter
                .as_ref()
                .map(|f| format!(" matching filter '{}'", f))
                .unwrap_or_default()
        ));
    }

    if source_has_main(&source) {
        return Err(format!(
            "'{}': test files must not declare 'main'; use @test functions instead",
            path.display()
        ));
    }

    let runner_source = synthesize_runner(&source, &tests);
    let out_dir = test_cache_dir(path, opts.release);
    fs::create_dir_all(&out_dir).map_err(|e| format!("create {}: {}", out_dir.display(), e))?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("tests");
    let runner_path = out_dir.join(format!("{stem}.runner.dream"));
    fs::write(&runner_path, &runner_source)
        .map_err(|e| format!("write {}: {}", runner_path.display(), e))?;

    let mut compiler = Compiler::new(Target::NativeC)
        .with_release(opts.release)
        .with_crate_type(CrateType::Bin)
        .with_emit_abi(false);
    if let Some(level) = opts.optimize {
        compiler = compiler.with_optimize(Some(level));
    }
    let runner_str = runner_path.to_string_lossy().into_owned();
    debug!(
        "running {} ({} test(s))",
        path.display(),
        tests.len()
    );
    let c_path = out_dir.join(format!("{stem}.c"));
    let c_str = c_path.to_string_lossy().into_owned();
    compiler
        .compile(&runner_str, &c_str)
        .map_err(|e| format!("compile '{}': {}", path.display(), e))?;
    let cc_opt = OptLevel::from_cli(opts.release, opts.optimize);
    let bin = crate::execution::native_c::compile_native_c(&c_path, cc_opt, false)
        .map_err(|e| format!("cc '{}': {}", path.display(), e))?;
    crate::execution::native_c::run_native_bin(&bin, &c_str, &[])
        .map_err(|e| format!("'{}' failed: {}", path.display(), e))?;
    Ok(tests.len())
}

fn source_has_main(source: &str) -> bool {
    for line in source.lines() {
        let t = line.trim_start();
        if t.starts_with("fun main(") || t.starts_with("async fun main(") {
            return true;
        }
        if t.starts_with("public fun main(") || t.starts_with("public async fun main(") {
            return true;
        }
    }
    false
}

fn synthesize_runner(original: &str, tests: &[DiscoveredTest]) -> String {
    let mut out = String::new();
    if !original.contains("import system.testing") && !original.contains("import system.testing;") {
        if !original.contains("import system;") && !original.contains("import system\n") {
            out.push_str("import system;\n");
        }
        out.push_str("import system.testing;\n");
    }
    out.push_str(original);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str("fun main(): void {\n");
    for t in tests {
        out.push_str("    Test.run(\"");
        out.push_str(&escape_string(&t.name));
        out.push_str("\", ");
        out.push_str(&t.name);
        out.push_str(");\n");
    }
    out.push_str("}\n");
    out
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn test_cache_dir(test_file: &Path, release: bool) -> PathBuf {
    let source_dir = test_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let sub = if release { "release" } else { "debug" };
    let mut dir = source_dir.to_path_buf();
    loop {
        if dir.join("dream.toml").is_file() {
            return dir.join("target").join("test").join(sub);
        }
        if !dir.pop() {
            break;
        }
    }
    source_dir.join("target").join("test").join(sub)
}
