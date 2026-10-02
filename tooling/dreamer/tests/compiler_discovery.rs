use std::path::Path;
use std::process::Command;

fn binary_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn isolated_command(executable: &Path, home: &Path, project: &Path) -> Command {
    let mut command = Command::new(executable);
    command
        .current_dir(project)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("PATH", "")
        .env_remove("DREAM_BIN")
        .env_remove("DREAM_HOME")
        .env_remove("DREAMER_HOME");
    command
}

#[test]
fn project_and_ancestor_target_directories_cannot_supply_the_compiler() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let tools = root.path().join("tools");
    let project = root.path().join("project");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    dreamer::commands::init::run(&project, Some("lookup-test".into()), None, false).unwrap();
    let executable = tools.join(binary_name("dreamer"));
    std::fs::copy(env!("CARGO_BIN_EXE_dreamer"), &executable).unwrap();
    for directory in [&project, root.path()] {
        for profile in ["debug", "release"] {
            let target = directory.join("target").join(profile);
            std::fs::create_dir_all(&target).unwrap();
            std::fs::write(target.join(binary_name("dream")), []).unwrap();
        }
    }
    let output = isolated_command(&executable, &home, &project)
        .arg("build")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("could not find the `dream` compiler executable"),
        "{stderr}"
    );
}
