use dreamer::manifest::{Manifest, PackageType, RunTarget};
use std::path::Path;
use std::process::{Command, Output};

fn init(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .arg("init")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn no_arguments_initializes_console_in_current_directory() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("hello");
    std::fs::create_dir(&project).unwrap();
    success(&init(&project, &[]));
    let manifest = Manifest::load(&project.join("dream.toml")).unwrap();
    assert_eq!(manifest.package().unwrap().name, "hello");
    assert!(manifest.app.is_none());
    assert!(
        std::fs::read_to_string(project.join("src/main.dream"))
            .unwrap()
            .contains("Hello, world!")
    );
    assert!(!project.join("console").exists());
}

#[test]
fn canvas_presets_share_source_but_select_distinct_platforms() {
    let root = tempfile::tempdir().unwrap();
    let mut source = None;
    for (template, targets) in [
        ("desktop", vec!["native"]),
        ("mobile", vec!["android", "ios"]),
        ("app", vec!["native", "android", "ios"]),
    ] {
        success(&init(root.path(), &[template, template]));
        let project = root.path().join(template);
        let manifest = Manifest::load(&project.join("dream.toml")).unwrap();
        assert_eq!(manifest.package().unwrap().targets, targets);
        let config = manifest.app.as_ref().unwrap();
        assert_eq!(config.android_min_sdk, 26);
        assert_eq!(config.ios_min_os, "16.0");
        config.validate().unwrap();
        let text = std::fs::read_to_string(project.join("src/main.dream")).unwrap();
        assert!(text.contains("DrawingBackend.Gpu"));
        assert!(text.contains("window.scale_factor"));
        assert!(text.contains("screen.resize"));
        if let Some(source) = &source {
            assert_eq!(&text, source);
        } else {
            source = Some(text);
        }
        if template == "mobile" {
            assert!(manifest.run_target(None).is_err());
        } else {
            assert_eq!(manifest.run_target(None).unwrap(), RunTarget::Native);
        }
    }
}

#[test]
fn unnamed_mobile_initializes_current_directory_instead_of_a_folder_named_mobile() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("phone");
    std::fs::create_dir(&project).unwrap();
    success(&init(&project, &["mobile"]));
    let manifest = Manifest::load(&project.join("dream.toml")).unwrap();
    assert_eq!(manifest.package().unwrap().name, "phone");
    assert_eq!(manifest.package().unwrap().targets, vec!["android", "ios"]);
    assert!(!project.join("mobile").exists());
}

#[test]
fn other_templates_have_launchers_and_package_types() {
    let root = tempfile::tempdir().unwrap();
    for template in ["console", "web", "node", "lib", "test", "workspace"] {
        success(&init(root.path(), &[template, template]));
        let project = root.path().join(template);
        let manifest = Manifest::load(&project.join("dream.toml")).unwrap();
        match template {
            "web" => assert!(project.join("index.html").is_file()),
            "node" => assert!(project.join("run.mjs").is_file()),
            "lib" => {
                assert_eq!(manifest.package().unwrap().package_type, PackageType::Lib);
                assert!(project.join("src/lib.dream").is_file());
            }
            "test" => assert!(project.join("example.dream").is_file()),
            "workspace" => {
                assert!(manifest.is_virtual_workspace());
                assert!(project.join("app/src/main.dream").is_file());
                assert!(project.join("tests/example.dream").is_file());
            }
            _ => assert!(project.join("src/main.dream").is_file()),
        }
    }
}

#[test]
fn initializer_rejects_removed_flags_and_invalid_options_before_creating_files() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec!["mobile", "bad", "--runtime", "android,ios"],
        vec!["console", "bad", "--lib"],
        vec!["console", "bad", "--test"],
        vec!["console", "bad", "--workspace"],
        vec!["app", "bad", "--output", "cdylib"],
        vec!["unknown", "bad"],
        vec!["console", "../bad"],
    ] {
        let output = init(root.path(), &args);
        assert!(!output.status.success(), "{args:?}");
        assert_eq!(root.path().read_dir().unwrap().count(), 0, "{args:?}");
    }
}

#[test]
fn library_output_and_explicit_directory_work() {
    let root = tempfile::tempdir().unwrap();
    success(&init(
        root.path(),
        &["lib", "shared", "--output", "cdylib", "--dir", "elsewhere"],
    ));
    let project = root.path().join("elsewhere");
    let manifest = Manifest::load(&project.join("dream.toml")).unwrap();
    assert_eq!(manifest.package().unwrap().name, "shared");
    assert_eq!(
        manifest.lib.unwrap().output_type,
        dream_abi::library::LibraryKind::Cdylib
    );
    assert!(!root.path().join("shared").exists());
}

#[test]
fn repeat_init_preserves_modified_source_and_manifest() {
    let root = tempfile::tempdir().unwrap();
    success(&init(root.path(), &["app", "hello"]));
    let project = root.path().join("hello");
    let manifest = std::fs::read(project.join("dream.toml")).unwrap();
    std::fs::write(project.join("src/main.dream"), "user code\n").unwrap();
    assert!(!init(&project, &["app"]).status.success());
    assert_eq!(
        std::fs::read_to_string(project.join("src/main.dream")).unwrap(),
        "user code\n"
    );
    assert_eq!(std::fs::read(project.join("dream.toml")).unwrap(), manifest);
}

#[test]
fn mobile_only_build_and_run_never_fall_through_to_desktop_compilation() {
    let root = tempfile::tempdir().unwrap();
    success(&init(root.path(), &["mobile", "phone"]));
    let project = root.path().join("phone");
    for args in [
        vec!["build"],
        vec!["build", "--target", "android"],
        vec!["run", "--target", "ios"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_dreamer"))
            .args(&args)
            .current_dir(&project)
            .env("DREAM_BIN", root.path().join("missing-compiler"))
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("multiple hosts")
                || error.contains("mobile canvas app build/run is not available")
                || error.contains("mobile toolchain diagnosis found missing requirements"),
            "{args:?}: {error}"
        );
        assert!(!project.join("target/debug/main.bin").exists());
    }
}
