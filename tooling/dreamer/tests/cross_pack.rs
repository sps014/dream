use object::Object;
use std::path::{Path, PathBuf};
use std::process::Command;

fn checked(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A stand-in capability library: only its exported symbols matter to the link.
fn foreign_library(clang: &Path, triple: &str, source: &Path, out: &Path) {
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

#[test]
#[ignore = "requires workspace binaries, the pinned LLVM and mold, and DREAM_SYSROOT for the foreign Linux target"]
fn packages_foreign_elf_and_its_target_libraries() {
    let root = tempfile::tempdir().unwrap();
    let llvm = std::env::var_os("DREAM_LLVM")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap(),
            )
            .join(format!(
                ".dream/toolchains/llvm-{}/bin",
                dream_abi::toolchain::LLVM_VERSION
            ))
        });
    let clang = llvm.join(if cfg!(windows) { "clang.exe" } else { "clang" });
    let sysroot = std::env::var_os("DREAM_SYSROOT").expect("set DREAM_SYSROOT to a foreign Linux sysroot");
    let dream = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(if cfg!(windows) { "dream.exe" } else { "dream" });
    let (target, triple, arch) = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        ("linux-arm64", "aarch64-unknown-linux-gnu", "aarch64")
    } else {
        ("linux-x64", "x86_64-unknown-linux-gnu", "x86_64")
    };
    let targets = root.path().join("targets");
    let library = targets.join(triple).join("lib/libdream_host_core.so");
    std::fs::create_dir_all(library.parent().unwrap()).unwrap();
    let source = root.path().join("host.c");
    std::fs::write(&source, "void dream_host_core_abi_v3(void) {}\nint dream_host_bind_v3(void *context) {(void)context;return 0;}\n").unwrap();
    foreign_library(&clang, triple, &source, &library);
    let timezone_source = root.path().join("timezone.c");
    std::fs::write(&timezone_source, "void dream_host_timezone_abi_v3(void) {}\nvoid *dateLocalZoneName(void) { return 0; }\nint dateZoneOffsetMinutes(void *zone, long long millis) { (void)zone; (void)millis; return 0; }\n").unwrap();
    foreign_library(
        &clang,
        triple,
        &timezone_source,
        &library.with_file_name("libdream_host_timezone.so"),
    );
    let project = root.path().join("project");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("dream.toml"), "[package]\nname = \"foreign\"\nversion = \"0.1.0\"\ntype = \"bin\"\nentry = \"src/main.dream\"\n").unwrap();
    std::fs::write(
        project.join("src/main.dream"),
        "import system; fun main(): void { let zone = TimeZone.local; }",
    )
    .unwrap();
    checked(
        Command::new(env!("CARGO_BIN_EXE_dreamer"))
            .args(["pack", "--target", target, "-O0"])
            .current_dir(&project)
            .env("DREAM_SYSROOT", &sysroot)
            .env("DREAM_TARGETS", targets)
            .env("DREAM_BIN", dream),
    );
    let bundle = project.join("target/pack").join(target);
    let bytes = std::fs::read(bundle.join(format!("foreign-{target}"))).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(file.format(), object::BinaryFormat::Elf);
    assert_eq!(
        file.architecture(),
        if arch == "aarch64" {
            object::Architecture::Aarch64
        } else {
            object::Architecture::X86_64
        }
    );
    assert_eq!(
        std::fs::read(bundle.join("libdream_host_core.so")).unwrap(),
        std::fs::read(library).unwrap()
    );
}
