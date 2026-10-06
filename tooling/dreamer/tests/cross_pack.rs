use object::Object;
use std::path::PathBuf;
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
#[ignore = "requires workspace binaries, installed LLVM and Zig"]
fn packages_foreign_elf_and_its_target_libraries() {
    let root = tempfile::tempdir().unwrap();
    let zig = std::env::var_os("DREAM_ZIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap(),
            )
            .join(".dream/toolchains/zig-0.16.0")
            .join(if cfg!(windows) { "zig.exe" } else { "zig" })
        });
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
    std::fs::write(&source, "void dream_host_core_abi_v2(void) {}\nvoid dream_host_bind_v2(void *a, void *b, void *c) {(void)a;(void)b;(void)c;}\n").unwrap();
    checked(
        Command::new(&zig)
            .args([
                "cc",
                "-target",
                &format!("{arch}-linux-gnu"),
                "-shared",
                "-fPIC",
                "-Wl,-soname,libdream_host_core.so",
            ])
            .arg(source)
            .arg("-o")
            .arg(&library),
    );
    let unicode_source = root.path().join("unicode.c");
    std::fs::write(&unicode_source, "void dream_host_unicode_abi_v2(void) {}\nvoid *unicodeToLower(void *text) { return text; }\n").unwrap();
    checked(
        Command::new(&zig)
            .args([
                "cc",
                "-target",
                &format!("{}-linux-gnu", arch),
                "-shared",
                "-fPIC",
                "-Wl,-soname,libdream_host_unicode.so",
            ])
            .arg(&unicode_source)
            .arg("-o")
            .arg(library.with_file_name("libdream_host_unicode.so")),
    );
    let project = root.path().join("project");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("dream.toml"), "[package]\nname = \"foreign\"\nversion = \"0.1.0\"\ntype = \"bin\"\nentry = \"src/main.dream\"\n").unwrap();
    std::fs::write(
        project.join("src/main.dream"),
        "import system.text; fun main(): void { let x = Unicode.to_lower_unicode(\"X\"); }",
    )
    .unwrap();
    checked(
        Command::new(env!("CARGO_BIN_EXE_dreamer"))
            .args(["pack", "--target", target, "-O0"])
            .current_dir(&project)
            .env("DREAM_CC", &zig)
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
