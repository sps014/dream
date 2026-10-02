#![cfg(all(feature = "native", any(target_os = "linux", target_os = "macos")))]

use dream::driver::compiler::Compiler;
use dream::driver::wasm_opt::OptLevel;
use dream::execution::native::compile_and_capture_ex;
use dream_mir::backend::Target;

#[test]
fn native_link_removes_unused_foreign_functions_but_keeps_called_symbols() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    std::fs::create_dir(root.join("native")).unwrap();
    std::fs::write(root.join("dream.toml"), "[package]\nname = \"size\"\n[native.size]\ncflags = [\"-ffunction-sections\", \"-fdata-sections\"]\n").unwrap();
    std::fs::write(
        root.join("native/size.c"),
        "int dream_size_used(void) { return 42; }\nint dream_size_unused(void) { return 99; }\n",
    )
    .unwrap();
    let source = root.join("main.dream");
    std::fs::write(&source, "import system;\n@c(\"size\", \"dream_size_used\") extern fun used(): int;\nfun main(): void { System.println(used()); }\n").unwrap();
    let module = root.join("main.ll");
    Compiler::new(Target::native())
        .compile(&source.display().to_string(), &module.display().to_string())
        .unwrap();
    let output = compile_and_capture_ex(
        &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        module.to_str().unwrap(),
        OptLevel::O0,
        &[],
        &[],
        None,
        60,
    )
    .unwrap();
    assert_eq!(output.trim(), "42");
    let symbols = std::process::Command::new("nm")
        .arg(module.with_extension("bin"))
        .output()
        .unwrap();
    assert!(symbols.status.success());
    let symbols = String::from_utf8(symbols.stdout).unwrap();
    assert!(symbols.contains("dream_size_used"));
    assert!(!symbols.contains("dream_size_unused"));
    let flags = std::fs::read_to_string(module.with_extension("flags")).unwrap();
    assert!(flags.contains(if cfg!(target_os = "macos") {
        "-Wl,-dead_strip"
    } else {
        "-Wl,--gc-sections"
    }));
}
