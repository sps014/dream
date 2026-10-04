#![cfg(feature = "native")]

use dream_abi::target::TargetSpec;
use object::Object;
use std::process::Command;

fn checked(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires installed Zig cross compiler and LLVM"]
fn zig_links_foreign_executable_and_rejects_stale_capability_abi() {
    let tools = std::env::var_os("DREAM_ZIG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home =
                std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap();
            std::path::PathBuf::from(home)
                .join(".dream/toolchains/zig-0.16.0")
                .join(if cfg!(windows) { "zig.exe" } else { "zig" })
        });
    assert!(
        tools.is_file(),
        "set DREAM_ZIG to the installed Zig executable"
    );
    let triple = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "aarch64-unknown-linux-gnu"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let spec = TargetSpec::parse(triple).unwrap();
    assert!(!spec.can_link_on_host());
    let root = tempfile::tempdir().unwrap();
    let targets = root.path().join("targets");
    let libs = targets.join(triple).join("lib");
    std::fs::create_dir_all(&libs).unwrap();
    let c = root.path().join("host.c");
    let library = libs.join("libdream_host_core.so");
    let build_host = |marker: &str| {
        std::fs::write(&c, format!("void {marker}(void) {{}}\nvoid dream_host_bind_v2(void *a, void *b, void *c) {{ (void)a; (void)b; (void)c; }}\n")).unwrap();
        checked(
            Command::new(&tools)
                .args([
                    "cc",
                    "-target",
                    &format!("{}-linux-gnu", spec.triple.architecture),
                    "-shared",
                    "-fPIC",
                    "-Wl,-soname,libdream_host_core.so",
                ])
                .arg(&c)
                .arg("-o")
                .arg(&library),
        );
    };
    build_host("dream_host_core_abi_v2");
    let source = root.path().join("main.dream");
    let ir = root.path().join("main.ll");
    std::fs::write(&source, "fun main(): void {}").unwrap();
    let compile = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dream"));
        command
            .args(["--target", triple, "-O0", "-o"])
            .arg(&ir)
            .arg(&source)
            .env("DREAM_CC", &tools)
            .env("DREAM_TARGETS", &targets);
        if !root.path().join("dream.toml").exists() {
            command.arg("--relocatable");
        }
        command
    };
    checked(&mut compile());
    let bytes = std::fs::read(ir.with_extension("bin")).unwrap();
    let binary = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(binary.format(), object::BinaryFormat::Elf);
    assert_eq!(
        binary.architecture(),
        if triple.starts_with("aarch64") {
            object::Architecture::Aarch64
        } else {
            object::Architecture::X86_64
        }
    );
    assert!(root.path().join("libdream_host_core.so").is_file());
    let first = std::fs::read_to_string(ir.with_extension("flags")).unwrap();
    assert!(first.contains(triple));
    build_host("dream_host_core_abi_v1");
    let output = compile().output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing current ABI marker"));
    build_host("dream_host_core_abi_v2");
    std::fs::write(&source, "@export fun twice(x: int): int { return x * 2; }").unwrap();
    for (kind, extension) in [("staticlib", "a"), ("cdylib", "so")] {
        std::fs::write(
            root.path().join("dream.toml"),
            format!(
                "[package]\nname = \"foreign\"\ntype = \"lib\"\n[lib]\noutput-type = \"{kind}\"\n"
            ),
        )
        .unwrap();
        checked(&mut compile());
        assert!(ir.with_extension(extension).is_file());
        let cached = compile().output().unwrap();
        assert!(cached.status.success());
        assert!(!String::from_utf8_lossy(&cached.stderr).contains("main.lib"));
        assert!(ir.with_extension("dream-cache").is_file());
    }
}
