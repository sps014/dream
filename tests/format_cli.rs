use std::{fs, process::Command};

#[test]
fn check_reports_skipped_input_as_failure() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("broken.dream");
    let source = "fun main() { ` }";
    fs::write(&file, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dream"))
        .args(["fmt", "--check"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(file).unwrap(), source);
}

#[test]
fn directory_formatting_skips_build_outputs_and_checks_cleanly_afterward() {
    let dir = tempfile::tempdir().unwrap();
    let source = "fun main(){return;}";
    fs::write(dir.path().join("main.dream"), source).unwrap();
    for name in ["target", ".dream", "node_modules", ".git", "site"] {
        fs::create_dir(dir.path().join(name)).unwrap();
        fs::write(dir.path().join(name).join("generated.dream"), source).unwrap();
    }
    let run = |check: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dream"));
        command.arg("fmt");
        if check {
            command.arg("--check");
        }
        command.arg(dir.path()).output().unwrap()
    };
    assert!(!run(true).status.success());
    assert!(run(false).status.success());
    assert!(run(true).status.success());
    assert_eq!(
        fs::read_to_string(dir.path().join("main.dream")).unwrap(),
        dream_format::format(source)
    );
    for name in ["target", ".dream", "node_modules", ".git", "site"] {
        assert_eq!(
            fs::read_to_string(dir.path().join(name).join("generated.dream")).unwrap(),
            source
        );
    }
}

#[test]
#[cfg(unix)]
fn directory_formatting_does_not_follow_symlink_cycles() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("main.dream"), "fun main() {}\n").unwrap();
    std::os::unix::fs::symlink(dir.path(), dir.path().join("cycle")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dream"))
        .args(["fmt", "--check"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
}
