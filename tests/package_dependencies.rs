//! A dependency package that brings its own C sources, JavaScript module and generator
//! (`tests/fixtures/packages/native_demo`), installed the way dreamer installs a Git dependency:
//! `dream_packages/<name>` is a link to a directory outside the program.

use dream::driver::compiler::Compiler;
use dream::driver::js_runtime::JsRuntimeTarget;
use dream::driver::wasm_opt::OptLevel;
use dream::execution::native::compile_and_capture_ex;
use dream_mir::backend::Target;
use std::path::{Path, PathBuf};

use crate::common;

const APP: &str = r#"import system;
import native_demo;

@demo_stamp
class Thing {}

@cfg(Target.Native)
fun twice(value: int): int {
    return value * 2;
}

@cfg(Target.Native)
fun target_line(): string {
    return demo_apply_plus_one(twice, 20).to_string();
}

@cfg(Target.Wasm)
fun target_line(): string {
    return "wasm";
}

fun main(): void {
    System.println(demo_sum(2, 3));
    System.println(Thing().demo_stamp());
    System.println(target_line());
}
"#;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/packages/native_demo")
}

fn app() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("dream.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nentry = \"main.dream\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("main.dream"), APP).unwrap();
    let packages = dir.path().join("dream_packages");
    std::fs::create_dir_all(&packages).unwrap();
    link_dir(&fixture(), &packages.join("native_demo"));
    dir
}

#[cfg(unix)]
fn link_dir(src: &Path, dest: &Path) {
    std::os::unix::fs::symlink(src, dest).unwrap();
}

#[cfg(windows)]
fn link_dir(src: &Path, dest: &Path) {
    junction::create(src, dest).unwrap();
}

fn compile(compiler: Compiler, entry: &Path, out: &Path) {
    compiler
        .with_render_diagnostics(false)
        .compile(&entry.to_string_lossy().to_string(), &out.to_string_lossy())
        .map_err(|e| e.diagnostic_text().unwrap_or(&e.to_string()).to_string())
        .unwrap();
}

#[test]
fn native_dependency_brings_c_sources_and_a_generator() {
    let dir = app();
    let ll = dir.path().join("main.ll");
    compile(
        Compiler::new(Target::native()),
        &dir.path().join("main.dream"),
        &ll,
    );
    let out = compile_and_capture_ex(
        &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        ll.to_str().unwrap(),
        OptLevel::O0,
        &[],
        &[],
        None,
        60,
    )
    .map(common::normalize_stdout)
    .unwrap();
    assert_eq!(out.trim(), "5\n7\n41");
}

#[test]
fn dependency_panics_report_the_calling_line() {
    let dir = app();
    let entry = dir.path().join("pick.dream");
    std::fs::write(
        &entry,
        "import native_demo;\n\nfun main(): void {\n    let values = [1, 2, 3];\n    demo_pick(values, 5);\n}\n",
    )
    .unwrap();
    let ll = dir.path().join("pick.ll");
    compile(Compiler::new(Target::native()), &entry, &ll);
    let error = compile_and_capture_ex(
        &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        ll.to_str().unwrap(),
        OptLevel::O0,
        &[],
        &[],
        None,
        60,
    )
    .expect_err("an out-of-range pick panics")
    .to_string();
    assert!(
        error.contains("index out of bounds") && error.contains("pick.dream:5"),
        "{error}"
    );
}

#[test]
fn wasm_dependency_uses_its_javascript_module() {
    let dir = app();
    let wasm = dir.path().join("main.wasm");
    compile(
        Compiler::new(Target::wasm32()).with_runtimes(vec![JsRuntimeTarget::Node]),
        &dir.path().join("main.dream"),
        &wasm,
    );
    let runner = dir.path().join("run.mjs");
    std::fs::write(
        &runner,
        "import {run} from './main.node.runtime.mjs';\n\
         await run(new URL('./main.wasm', import.meta.url), {stdout: value => process.stdout.write(value)});\n",
    )
    .unwrap();
    let result = std::process::Command::new("node")
        .arg(&runner)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(String::from_utf8(result.stdout).unwrap(), "5\n7\nwasm\n");
}
