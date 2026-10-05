#![cfg(feature = "native")]

use dream::driver::output::OutputKind;
use std::fs;
use std::path::Path;
use std::process::Command;

fn command(mut cmd: Command) -> std::process::Output {
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{cmd:?}\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn build(root: &Path, kind: OutputKind, extra: &[&str]) -> std::path::PathBuf {
    let ll = root.join("lib.ll");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dream"));
    let manifest_path = root.join("dream.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    let package = manifest.split("[lib]").next().unwrap();
    fs::write(
        &manifest_path,
        format!(
            "{package}\n[lib]\noutput-type = \"{}\"\n",
            if kind == OutputKind::Staticlib {
                "staticlib"
            } else {
                "cdylib"
            }
        ),
    )
    .unwrap();
    cmd.arg(root.join("src/lib.dream"))
        .arg("-o")
        .arg(kind.artifact_path(&ll, &dream_abi::target::TargetSpec::host()))
        .args(extra);
    command(cmd);
    let manifest = dream_abi::host_capability::HostManifest::parse(
        &fs::read_to_string(ll.with_extension("abi.json")).unwrap(),
    )
    .unwrap();
    if manifest.host_capabilities.is_empty() {
        let ir = fs::read_to_string(ll.with_extension("opt.ll")).unwrap();
        assert!(!ir.contains("call void @dream_host_bind_v2("));
        if kind == OutputKind::Staticlib {
            assert!(!fs::read_to_string(ll.with_extension("link.json"))
                .unwrap()
                .contains("dream_host"));
        }
    }
    kind.artifact_path(&ll, &dream_abi::target::TargetSpec::host())
}

fn consumer(root: &Path, product: &Path, kind: OutputKind, source: &str) -> std::path::PathBuf {
    let c = root.join("consumer.c");
    fs::write(&c, source).unwrap();
    let exe = root.join(if cfg!(windows) {
        "consumer.exe"
    } else {
        "consumer"
    });
    let config = std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default());
    let tools = dream::execution::llvm::resolve_llvm(&config).unwrap();
    let mut cc = Command::new(if cfg!(windows) {
        tools.clang().unwrap()
    } else {
        "cc".into()
    });
    cc.arg(&c);
    if cfg!(windows) && kind == OutputKind::Dylib {
        cc.arg(product.with_extension("lib"));
    } else {
        cc.arg(product);
    }
    if kind == OutputKind::Staticlib {
        let flags: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("lib.link.json")).unwrap()).unwrap();
        cc.args(
            flags["link_args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a.as_str().unwrap()),
        );
    } else if !cfg!(windows) {
        cc.arg(format!("-Wl,-rpath,{}", root.display()));
    }
    cc.arg("-o").arg(&exe);
    command(cc);
    exe
}

fn run_consumer(exe: &Path) -> Command {
    let mut cmd = Command::new(exe);
    if cfg!(windows) {
        let config = dream::driver::toolchain::ToolchainConfig::default();
        let mut paths = config.host_library_dirs();
        paths.push(exe.parent().unwrap().to_path_buf());
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        cmd.env("PATH", std::env::join_paths(paths).unwrap());
    }
    cmd
}

fn project(root: &Path, source: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("dream.toml"),
        "[package]\nname = \"mylib\"\ntype = \"lib\"\n",
    )
    .unwrap();
    fs::write(root.join("src/lib.dream"), source).unwrap();
}

#[test]
fn c_links_both_library_kinds_and_uses_plain_exports() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("mylib");
    project(
        &root,
        r#"
import system;
let base: int = 40;
@export fun add(a: int, b: int): int { return base + a + b; }
@export fun wide(x: long): long { return x + 1; }
@export fun real(x: double): double { return x * 2.0; }
@export fun hello(): string { return "hello"; }
struct Pair { public x: int; public constructor(x: int) { this.x = x; } }
@export fun pair(x: int): Pair { return Pair(x); }
@export fun readPair(ref p: Pair): int { return p.x; }
class Tracked { public constructor() {} del() { System.println("destroyed"); } }
@export fun tracked(): Tracked { return Tracked(); }
@export fun discard(x: Tracked): void {}
@c extern fun c_add(a: int, b: int): int;
@export fun nativeAdd(a: int, b: int): int { return c_add(a, b); }
fun privateHelper(): int { return 999; }
"#,
    );
    fs::create_dir_all(root.join("native")).unwrap();
    fs::write(
        root.join("native/bridge.c"),
        "#include <stdint.h>\nint32_t c_add(int32_t a, int32_t b) { return a + b; }\n",
    )
    .unwrap();
    for kind in [OutputKind::Staticlib, OutputKind::Dylib] {
        let product = build(&root, kind, &["--release"]);
        let ir = fs::read_to_string(root.join("lib.opt.ll")).unwrap();
        assert!(!ir
            .lines()
            .any(|l| l.starts_with("define ") && l.contains("@main(")));
        assert!(!ir.contains("privateHelper"));
        let header = fs::read_to_string(root.join("lib.h")).unwrap();
        assert!(header.contains("int32_t add(int32_t arg0, int32_t arg1);"));
        assert!(!header.contains(temp.path().to_str().unwrap()));
        let exe = consumer(
            &root,
            &product,
            kind,
            r#"
#include "lib.h"
#include <assert.h>
int main(void) {
    dream_thread_attach();
    assert(add(1, 1) == 42);
    assert(add(2, 3) == 45);
    assert(wide(4294967296LL) == 4294967297LL);
    assert(real(1.5) == 3.0);
    void *s = hello(); dream_retain(s); dream_release(s); dream_release(s);
    void *p = pair(17); assert(readPair(p) == 17); dream_release(p);
    void *t = tracked(); discard(t);
    assert(nativeAdd(20, 22) == 42);
    dream_thread_detach();
    return 0;
}
"#,
        );
        let out = command(run_consumer(&exe));
        assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "destroyed");
        if !cfg!(windows) {
            let mut nm = Command::new("nm");
            nm.arg(if cfg!(target_os = "macos") {
                "-g"
            } else {
                "--defined-only"
            })
            .arg(&product);
            let symbols = String::from_utf8(command(nm).stdout).unwrap();
            for name in [
                "add",
                "dream_thread_attach",
                "dream_thread_detach",
                "dream_retain",
                "dream_release",
                "dream_set_panic_hook",
            ] {
                assert!(
                    symbols.lines().any(|l| l.ends_with(name)),
                    "missing {}: {}",
                    name,
                    symbols
                );
            }
        }
    }
}

#[test]
fn library_panic_locations_are_relative_and_belong_to_the_library() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("mylib");
    project(&root, "import system;\nimport system.collections;\nimport dep.helper;\nfun helper(): void {\n    System.panic(\"own\");\n}\n@export fun own(): void { helper(); }\n@export fun std(): void {\n    let xs = List<int>();\n    xs[0];\n}\n@export fun dependency(): void {\n    depFail();\n}\n");
    let dep = root.join("dream_packages/dep/src");
    fs::create_dir_all(&dep).unwrap();
    fs::write(dep.join("helper.dream"), "module dep.helper;\nimport system;\npublic fun depFail(): void { System.panic(\"dep\"); }\n").unwrap();
    fs::write(
        root.join("dream_packages/dep/dream.toml"),
        "[package]\nname = \"dep\"\n",
    )
    .unwrap();
    for kind in [OutputKind::Staticlib, OutputKind::Dylib] {
        let product = build(&root, kind, &["--release"]);
        let ir = fs::read_to_string(root.join("lib.opt.ll")).unwrap();
        assert!(!ir.contains(temp.path().to_str().unwrap()));
        let exe = consumer(
            &root,
            &product,
            kind,
            r#"
#include "lib.h"
#include <stdio.h>
static void hook(const char *message, const char *location) {
    (void)message; printf("LOCATION:%s\n", location); fflush(stdout);
}
int main(int argc, char **argv) {
    (void)argc;
    dream_thread_attach(); dream_set_panic_hook(hook);
    if (argv[1][0] == 'o') own();
    else if (argv[1][0] == 's') std();
    else dependency();
    return 0;
}
"#,
        );
        for (arg, line) in [("own", 5), ("std", 10), ("dep", 13)] {
            let out = run_consumer(&exe).arg(arg).output().unwrap();
            assert!(!out.status.success());
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(
                stdout.contains(&format!("LOCATION:mylib/src/lib.dream:{line}")),
                "{stdout}\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

#[test]
fn moving_the_package_preserves_library_ir_and_headers() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a/mylib");
    let b = temp.path().join("b/mylib");
    let source = "import system;\n@export fun fail(): void { System.panic(\"boom\"); }\n";
    project(&a, source);
    project(&b, source);
    build(&a, OutputKind::Staticlib, &[]);
    build(&b, OutputKind::Staticlib, &[]);
    let archive = OutputKind::Staticlib
        .artifact_path(Path::new("lib.ll"), &dream_abi::target::TargetSpec::host());
    for file in [Path::new("lib.opt.ll"), Path::new("lib.h"), &archive] {
        assert!(
            fs::read(a.join(file)).unwrap() == fs::read(b.join(file)).unwrap(),
            "{} differs",
            file.display()
        );
    }
}

#[test]
fn manifest_selects_default_source_and_rejects_array_outputs() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(
        root.join("src/mylib.dream"),
        "@export fun answer(): int { return 42; }",
    )
    .unwrap();
    let manifest =
        "[package]\nname = \"mylib\"\ntype = \"lib\"\n[lib]\noutput-type = \"staticlib\"\n";
    fs::write(root.join("dream.toml"), manifest).unwrap();
    let ll = root.join("out.ll");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dream"));
    cmd.current_dir(root.join("src"))
        .arg("build")
        .arg("-o")
        .arg(&ll);
    command(cmd);
    assert!(OutputKind::Staticlib
        .artifact_path(&ll, &dream_abi::target::TargetSpec::host())
        .is_file());
    fs::write(
        root.join("dream.toml"),
        manifest.replace("\"staticlib\"", "[\"staticlib\"]"),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dream"))
        .current_dir(root)
        .arg("build")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[lib]"));
}

#[test]
fn optional_service_binding_works_in_static_and_shared_libraries() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("service");
    project(
        &root,
        r#"
        import system.text;
        @export fun normalized_len(): int { return Unicode.normalize("é", UnicodeNormForm.Nfc).length; }
    "#,
    );
    for kind in [OutputKind::Staticlib, OutputKind::Dylib] {
        let product = build(&root, kind, &["--release"]);
        let manifest = dream_abi::host_capability::HostManifest::parse(
            &fs::read_to_string(root.join("lib.abi.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            manifest.host_capabilities,
            vec![
                dream_abi::host_capability::HostCapability::Core,
                dream_abi::host_capability::HostCapability::Unicode
            ]
        );
        let exe = consumer(&root, &product, kind, "#include \"lib.h\"\n#include <assert.h>\nint main(void) { dream_thread_attach(); assert(normalized_len() == 1); dream_thread_detach(); return 0; }\n");
        command(run_consumer(&exe));
    }
}
