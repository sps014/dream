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

/// The pinned clang and the foreign target's sysroot, which every cross link needs.
fn cross_toolchain() -> (std::path::PathBuf, std::ffi::OsString) {
    let bin = std::env::var_os("DREAM_LLVM")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home =
                std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap();
            std::path::PathBuf::from(home).join(format!(
                ".dream/toolchains/llvm-{}/bin",
                dream_abi::toolchain::LLVM_VERSION
            ))
        });
    let clang = bin.join(if cfg!(windows) { "clang.exe" } else { "clang" });
    assert!(clang.is_file(), "set DREAM_LLVM to the pinned LLVM bin/");
    let sysroot = std::env::var_os("DREAM_SYSROOT")
        .unwrap_or_else(|| panic!("set DREAM_SYSROOT to a {} sysroot", foreign_triple()));
    (clang, sysroot)
}

/// A stand-in capability library: only its exported symbols matter to the link.
fn foreign_library(
    clang: &std::path::Path,
    triple: &str,
    source: &std::path::Path,
    out: &std::path::Path,
) {
    let name = out.file_name().unwrap().to_string_lossy();
    checked(
        Command::new(clang)
            .arg(format!("--target={triple}"))
            .args(["-fuse-ld=lld", "-nostdlib", "-shared", "-fPIC"])
            .arg(format!("-Wl,-soname,{name}"))
            .arg(source)
            .arg("-o")
            .arg(out),
    );
}

fn foreign_triple() -> &'static str {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "aarch64-unknown-linux-gnu"
    } else {
        "x86_64-unknown-linux-gnu"
    }
}

#[test]
#[ignore = "requires the pinned LLVM and mold, and DREAM_SYSROOT for the foreign Linux target"]
fn links_foreign_release_executable_with_local_exec_runtime_tls() {
    let (_, sysroot) = cross_toolchain();
    let triple = foreign_triple();
    let root = tempfile::tempdir().unwrap();
    let ir = root.path().join("classes.ll");
    checked(
        Command::new(env!("CARGO_BIN_EXE_dream"))
            .args(["--release", "--emit-opt-ir", "--target", triple, "-o"])
            .arg(&ir)
            .arg("tests/fixtures/compiler/classes.dream")
            .env("DREAM_SYSROOT", &sysroot),
    );
    let bytes = std::fs::read(ir.with_extension("bin")).unwrap();
    let binary = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(binary.format(), object::BinaryFormat::Elf);
    let optimized = std::fs::read_to_string(ir.with_extension("opt.ll")).unwrap();
    assert!(optimized.contains("internal thread_local"));
    assert!(optimized.contains(r#"!{i32 7, !"PIE Level", i32 2}"#));
}

#[test]
#[ignore = "requires the pinned LLVM and mold, and DREAM_SYSROOT for the foreign Linux target"]
fn links_foreign_executable_and_rejects_stale_capability_abi() {
    let (clang, sysroot) = cross_toolchain();
    let triple = foreign_triple();
    let spec = TargetSpec::parse(triple).unwrap();
    assert!(!spec.can_link_on_host());
    let root = tempfile::tempdir().unwrap();
    let targets = root.path().join("targets");
    let libs = targets.join(triple).join("lib");
    std::fs::create_dir_all(&libs).unwrap();
    let c = root.path().join("host.c");
    let library = libs.join("libdream_host_core.so");
    let build_host = |marker: &str| {
        std::fs::write(&c, format!("void {marker}(void) {{}}\nint dream_host_bind_v3(void *context) {{ (void)context; return 0; }}\n")).unwrap();
        foreign_library(&clang, triple, &c, &library);
    };
    let timezone_source = root.path().join("timezone.c");
    std::fs::write(&timezone_source, "void dream_host_timezone_abi_v3(void) {}\nvoid *dateLocalZoneName(void) { return 0; }\nint dateZoneOffsetMinutes(void *zone, long long millis) { (void)zone; (void)millis; return 0; }\n").unwrap();
    foreign_library(
        &clang,
        triple,
        &timezone_source,
        &libs.join("libdream_host_timezone.so"),
    );
    build_host("dream_host_core_abi_v3");
    let source = root.path().join("main.dream");
    let ir = root.path().join("main.ll");
    std::fs::write(
        &source,
        "import system; fun main(): void { let zone = TimeZone.local; }",
    )
    .unwrap();
    let compile = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dream"));
        command
            .args(["--target", triple, "-o"])
            .arg(&ir)
            .arg(&source)
            .env("DREAM_SYSROOT", &sysroot)
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
    build_host("dream_host_core_abi_v3");
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
