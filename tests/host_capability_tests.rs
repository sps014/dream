#![cfg(feature = "native")]

use dream::driver::compiler::{Compiler, Target};
use dream_abi::host_capability::{HostCapability, HostManifest};

fn inventory(source: &str) -> Vec<HostCapability> {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("main.dream");
    let output = directory.path().join("main.ll");
    std::fs::write(&input, source).unwrap();
    Compiler::new(Target::Native)
        .compile(&input.display().to_string(), &output.display().to_string())
        .unwrap();
    HostManifest::parse(&std::fs::read_to_string(output.with_extension("abi.json")).unwrap())
        .unwrap()
        .host_capabilities
}

#[test]
fn unused_capability_imports_and_functions_do_not_link_hosts() {
    assert_eq!(
        inventory(
            r#"
        import system;
        import system.net;
        import system.gpu;
        import system.webview;
        fun unused(): bool { return Gpu.is_available; }
        fun main(): void { System.println("hello"); }
    "#
        ),
        vec![HostCapability::Core]
    );
}

#[test]
fn live_hosts_select_only_their_own_capabilities() {
    for (stem, capability) in [
        ("http_stream_connect_fail", HostCapability::Net),
        ("webapi_tls", HostCapability::Net),
        ("gpu_capabilities", HostCapability::Gpu),
    ] {
        let source = std::fs::read_to_string(format!("tests/cases/{stem}.dream")).unwrap();
        assert_eq!(inventory(&source), vec![HostCapability::Core, capability]);
    }
    assert_eq!(
        inventory(
            r#"
        import system.desktop;
        fun main(): void { Shell.open(""); }
    "#
        ),
        vec![HostCapability::Core, HostCapability::WebView]
    );
}

#[test]
fn cpu_only_gpu_helpers_do_not_require_gpu_host() {
    let source = std::fs::read_to_string("tests/cases/gpu_math_log_cpu.dream").unwrap();
    assert_eq!(inventory(&source), vec![HostCapability::Core]);
}

#[cfg(unix)]
#[test]
fn core_only_toolchain_compiles_and_runs_without_optional_hosts() {
    let tools = dream::execution::llvm::resolve_llvm(&std::sync::Arc::new(
        dream::driver::toolchain::ToolchainConfig::default(),
    ))
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let compiler = std::path::Path::new(env!("CARGO_BIN_EXE_dream"));
    let isolated = directory.path().join("dream");
    std::fs::copy(compiler, &isolated).unwrap();
    std::fs::copy(
        compiler.with_file_name(HostCapability::Core.library_name()),
        directory.path().join(HostCapability::Core.library_name()),
    )
    .unwrap();
    let source = directory.path().join("main.dream");
    std::fs::write(
        &source,
        r#"import system; fun main(): void { System.println("core only"); }"#,
    )
    .unwrap();
    let output = std::process::Command::new(&isolated)
        .arg("run")
        .arg(&source)
        .arg("-O0")
        .env("DREAM_LLVM", tools.bin)
        .env("DREAM_HOME", directory.path())
        .env("DREAM_BIN", &isolated)
        .env("HOME", directory.path())
        .env("DREAM_CC", "/usr/bin/cc")
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("DYLD_LIBRARY_PATH")
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("core only"));
}
