#![cfg(feature = "native")]

use dream::driver::compiler::Compiler;
use dream_abi::host_capability::{HostCapability, HostManifest};
use dream_mir::backend::Target;

fn inventory(source: &str) -> Vec<HostCapability> {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("main.dream");
    let output = directory.path().join("main.ll");
    std::fs::write(&input, source).unwrap();
    Compiler::new(Target::native())
        .compile(&input.display().to_string(), &output.display().to_string())
        .unwrap();
    let manifest =
        HostManifest::parse(&std::fs::read_to_string(output.with_extension("abi.json")).unwrap())
            .unwrap();
    let ir = std::fs::read_to_string(&output).unwrap();
    assert_eq!(
        ir.contains("call void @dream_host_bind_v2("),
        !manifest.host_capabilities.is_empty()
    );
    manifest.host_capabilities
}

#[test]
fn unused_capability_imports_and_functions_do_not_link_hosts() {
    assert_eq!(
        inventory(
            r#"
        import system;
        import system.crypto;
        import system.text;
        fun unused(): byte[] { return SecureRandom.bytes(1); }
        fun main(): void { System.println("hello"); }
    "#
        ),
        vec![]
    );
}

#[cfg(unix)]
#[test]
fn compiler_without_any_host_libraries_compiles_and_runs() {
    let tools = dream::execution::llvm::resolve_llvm(&std::sync::Arc::new(
        dream::driver::toolchain::ToolchainConfig::default(),
    ))
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let compiler = std::path::Path::new(env!("CARGO_BIN_EXE_dream"));
    let isolated = directory.path().join("dream");
    std::fs::copy(compiler, &isolated).unwrap();
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

#[test]
fn optional_services_select_exact_libraries_and_bind_core() {
    for (source, capability) in [
        (
            r#"import system.text; fun main(): void { let x = Unicode.normalize("text", UnicodeNormForm.Nfc); }"#,
            HostCapability::Unicode,
        ),
        (
            r#"import system; fun main(): void { let x = TimeZone.local; }"#,
            HostCapability::Timezone,
        ),
    ] {
        assert_eq!(inventory(source), vec![HostCapability::Core, capability]);
    }
    for (stem, capability) in [
        ("crypto_basic", HostCapability::Crypto),
        ("process_run_basic", HostCapability::Process),
    ] {
        let source = std::fs::read_to_string(format!("tests/cases/{stem}.dream")).unwrap();
        assert_eq!(inventory(&source), vec![HostCapability::Core, capability]);
    }
}

#[test]
fn service_combinations_keep_one_core_and_canonical_order() {
    assert_eq!(
        inventory(
            r#"
        import system; import system.text; import system.crypto;
        fun main(): void {
            let text = Unicode.to_lower_unicode("X");
            let bytes = SecureRandom.bytes(1);
            let zone = TimeZone.local;
        }
    "#
        ),
        vec![
            HostCapability::Core,
            HostCapability::Unicode,
            HostCapability::Crypto,
            HostCapability::Timezone
        ]
    );
}
