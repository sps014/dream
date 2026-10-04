use super::*;
use dream_abi::exports::{ExportFunction, ExportKind as K, ExportParam, ExportType};
use std::path::PathBuf;
use std::process::Command;

fn functions() -> Vec<ExportFunction> {
    vec![
        ExportFunction {
            name: "pass_handle".into(),
            ret: ExportType {
                c_type: "void *".into(),
                kind: K::Opaque,
            },
            params: vec![ExportParam {
                ty: ExportType {
                    c_type: "void *".into(),
                    kind: K::Opaque,
                },
                take: false,
                is_ref: false,
            }],
        },
        ExportFunction {
            name: "add".into(),
            ret: ExportType {
                c_type: "int32_t".into(),
                kind: K::Int,
            },
            params: vec![ExportParam {
                ty: ExportType {
                    c_type: "int32_t".into(),
                    kind: K::Int,
                },
                take: false,
                is_ref: false,
            }],
        },
    ]
}

#[test]
fn bridge_metadata_rejects_invalid_types_names_and_java_packages() {
    let mut functions = functions();
    bridge::validate(&functions).unwrap();
    functions[0].ret.c_type = "void *); injected(".into();
    assert!(bridge::validate(&functions).is_err());
    functions[0].ret.c_type = "void *".into();
    functions[0].name = "invalid-name".into();
    assert!(bridge::validate(&functions).is_err());
    for package in ["", ".example", "a.class", "foo;bar", "a._"] {
        assert!(bridge::java_package(package).is_err());
    }
    bridge::java_package("org.example.my_library").unwrap();
}

#[test]
fn slices_require_matching_target_and_export_inventory() {
    let temp = tempfile::tempdir().unwrap();
    let write = |stem: &str, triple: &str, exports: &str| {
        let lib = temp.path().join(format!("{stem}.so"));
        std::fs::write(&lib, b"library").unwrap();
        std::fs::write(lib.with_extension("h"), b"header").unwrap();
        std::fs::write(
            lib.with_extension("abi.json"),
            format!(
                "{{\"target_triple\":\"{triple}\",\"exports\":{exports},\"export_functions\":{}}}",
                dream_abi::exports::to_json(&functions())
            ),
        )
        .unwrap();
        lib
    };
    let arm = write("arm", "aarch64-linux-android", "[\"pass_handle\",\"add\"]");
    let x64 = write("x64", "x86_64-linux-android", "[\"pass_handle\",\"add\"]");
    let mut slices = vec![
        format!("aarch64-linux-android={}", arm.display()),
        format!("x86_64-linux-android={}", x64.display()),
    ];
    assert_eq!(read_slices("android", &slices).unwrap().len(), 2);
    slices[1] = format!("x86_64-linux-android={}", arm.display());
    assert!(read_slices("android", &slices).is_err());
    assert!(read_slices("android", &slices[..1]).is_err());
    write("arm", "aarch64-linux-android", "[\"main\"]");
    assert!(read_slices("android", &slices).is_err());
}

#[test]
fn package_zip_bytes_and_entries_are_deterministic() {
    let temp = tempfile::tempdir().unwrap();
    let files = vec![
        ("jni/arm64-v8a/libdemo.so".into(), b"arm".to_vec()),
        ("AndroidManifest.xml".into(), b"manifest".to_vec()),
        ("jni/x86_64/libdemo.so".into(), b"x64".to_vec()),
    ];
    let first = temp.path().join("first.aar");
    let second = temp.path().join("second.aar");
    android::write_zip(&first, &files).unwrap();
    let mut reverse = files.clone();
    reverse.reverse();
    android::write_zip(&second, &reverse).unwrap();
    assert_eq!(
        std::fs::read(&first).unwrap(),
        std::fs::read(second).unwrap()
    );
    let mut archive = zip::ZipArchive::new(std::fs::File::open(first).unwrap()).unwrap();
    assert_eq!(archive.len(), 3);
    assert_eq!(archive.by_index(0).unwrap().name(), "AndroidManifest.xml");
}

#[test]
#[ignore = "requires macOS SDK and a JDK; run when validating mobile bridge generation"]
fn generated_bridges_compile_against_foundation_and_jni() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::write(root.join("dream_library.h"), "#include <stdint.h>\nvoid dream_thread_attach(void); void dream_thread_detach(void); void dream_release(void *); void *pass_handle(void *); int32_t add(int32_t);\n").unwrap();
    let (header, source) = bridge::objc("demo", &functions());
    std::fs::write(root.join("Dream_demo.h"), header).unwrap();
    std::fs::write(root.join("bridge.m"), source).unwrap();
    command(
        Command::new("xcrun")
            .args(["clang", "-fsyntax-only", "-fobjc-arc", "-Werror"])
            .arg("-I")
            .arg(root)
            .arg(root.join("bridge.m")),
        "checking Objective-C",
    )
    .unwrap();
    let (java, c) = bridge::jni("demo", "org.example.my_library", &functions());
    std::fs::write(root.join("DreamLibrary.java"), java).unwrap();
    std::fs::write(root.join("bridge.c"), c).unwrap();
    command(
        Command::new("javac")
            .args(["--release", "8", "-d"])
            .arg(root)
            .arg(root.join("DreamLibrary.java")),
        "checking Java",
    )
    .unwrap();
    let home = Command::new("/usr/libexec/java_home").output().unwrap();
    let include = PathBuf::from(String::from_utf8(home.stdout).unwrap().trim()).join("include");
    command(
        Command::new("cc")
            .args(["-fsyntax-only", "-Werror"])
            .arg("-I")
            .arg(&include)
            .arg("-I")
            .arg(include.join("darwin"))
            .arg("-I")
            .arg(root)
            .arg(root.join("bridge.c")),
        "checking JNI",
    )
    .unwrap();
    command(
        Command::new("javap")
            .args(["-classpath"])
            .arg(root)
            .arg("-s")
            .arg("org.example.my_library.DreamLibrary"),
        "checking Java class descriptors",
    )
    .unwrap();
}
