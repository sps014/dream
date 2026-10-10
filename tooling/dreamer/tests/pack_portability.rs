#![cfg(any(target_os = "macos", target_os = "linux"))]

use dream_abi::host_capability::HostCapability;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MESSAGE: &str = "packed without a Dream installation";

fn checked_text(command: &mut Command) -> String {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn compiler() -> PathBuf {
    let executable = std::env::current_exe().unwrap();
    let binary = executable.parent().unwrap().parent().unwrap().join("dream");
    assert!(
        binary.is_file(),
        "build the workspace before running pack portability: {}",
        binary.display()
    );
    binary
}

fn clean_output(executable: &Path, empty_home: &Path, working: &Path) -> Output {
    Command::new(executable)
        .env_clear()
        .env("HOME", empty_home)
        .env("PATH", "/usr/bin:/bin")
        .current_dir(working)
        .output()
        .unwrap()
}

fn assert_runs_clean(executable: &Path, empty_home: &Path, working: &Path) {
    let output = clean_output(executable, empty_home, working);
    assert!(
        output.status.success(),
        "{}: {}",
        executable.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), MESSAGE);
    assert_eq!(std::fs::read_dir(empty_home).unwrap().count(), 0);
}

fn bracket_value(line: &str) -> Option<&str> {
    line.split_once('[')?
        .1
        .split_once(']')
        .map(|(value, _)| value)
}

fn macos_rpaths(text: &str) -> Vec<&str> {
    let mut in_rpath = false;
    let mut paths = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with("cmd ") {
            in_rpath = line == "cmd LC_RPATH";
        }
        if in_rpath && let Some(path) = line.strip_prefix("path ") {
            paths.push(path.split_once(" (offset ").expect("otool rpath offset").0);
        }
    }
    paths
}

fn package_relative(path: &str) -> bool {
    [
        "$ORIGIN",
        "${ORIGIN}",
        "@executable_path",
        "@loader_path",
        "@rpath",
    ]
    .iter()
    .any(|prefix| {
        path == *prefix
            || path
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

fn assert_search_paths<'a>(paths: impl IntoIterator<Item = &'a str>) {
    for path in paths {
        assert!(
            package_relative(path) && !path.starts_with("@rpath"),
            "non-package loader search path: {path}"
        );
    }
}

fn inspect_loader(binary: &Path, executable: bool) {
    #[cfg(target_os = "macos")]
    {
        let libraries = checked_text(Command::new("otool").arg("-L").arg(binary));
        let dependencies: Vec<_> = libraries
            .lines()
            .skip(1)
            .map(|line| {
                line.trim()
                    .split_once(" (compatibility version")
                    .expect("otool dependency")
                    .0
            })
            .collect();
        assert!(dependencies.contains(&"@rpath/libdream_host_core.dylib"));
        if !executable {
            let own_name = format!("@rpath/{}", binary.file_name().unwrap().to_str().unwrap());
            assert!(dependencies.contains(&own_name.as_str()));
        }
        for dependency in dependencies {
            assert!(
                package_relative(dependency)
                    || dependency.starts_with("/System/Library/")
                    || dependency.starts_with("/usr/lib/"),
                "non-system absolute or ambiguous dependency: {dependency}"
            );
        }
        let commands = checked_text(Command::new("otool").arg("-l").arg(binary));
        let paths = macos_rpaths(&commands);
        assert_search_paths(paths.iter().copied());
        if executable {
            assert!(paths.contains(&"@executable_path"));
            assert!(paths.contains(&"@executable_path/../Frameworks"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        let dynamic = checked_text(Command::new("readelf").args(["-d", "--wide"]).arg(binary));
        let mut dependencies = Vec::new();
        let mut paths = Vec::new();
        let mut soname = None;
        for line in dynamic.lines() {
            if line.contains("(NEEDED)") {
                let dependency = bracket_value(line).expect("readelf dependency");
                assert!(
                    !dependency.contains('/'),
                    "absolute or relative-path ELF dependency: {dependency}"
                );
                dependencies.push(dependency);
            } else if line.contains("(RPATH)") || line.contains("(RUNPATH)") {
                paths.extend(bracket_value(line).expect("readelf search path").split(':'));
            } else if line.contains("(SONAME)") {
                soname = bracket_value(line);
            }
        }
        assert_search_paths(paths.iter().copied());
        if executable {
            assert!(dependencies.contains(&"libdream_host_core.so"));
            assert!(paths.contains(&"$ORIGIN"));
        } else {
            assert_eq!(soname, binary.file_name().unwrap().to_str());
            if binary.file_name().unwrap() != "libdream_host_core.so" {
                assert!(dependencies.contains(&"libdream_host_core.so"));
                assert!(paths.contains(&"$ORIGIN"));
            }
        }
    }
}

#[test]
fn packed_application_has_no_builder_runtime_dependency() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("source project");
    dreamer::commands::init::run(&project, Some("portable".into()), None, false).unwrap();
    std::fs::write(
        project.join("src/main.dream"),
        format!("import system;\nimport system.text;\nfun main(): void {{ System.println(Unicode.normalize(\"{MESSAGE}\", UnicodeNormForm.Nfc)); }}\n"),
    )
    .unwrap();
    checked_text(
        Command::new(env!("CARGO_BIN_EXE_dreamer"))
            .arg("pack")
            .arg("-O0")
            .env("DREAM_BIN", compiler())
            .current_dir(&project),
    );
    let moved = temporary.path().join("relocated package");
    std::fs::rename(
        project.join("target/pack").join(format!(
            "{}-{}",
            std::env::consts::OS,
            if std::env::consts::ARCH == "x86_64" {
                "x64"
            } else {
                "arm64"
            }
        )),
        &moved,
    )
    .unwrap();
    let empty_home = temporary.path().join("empty home");
    std::fs::create_dir(&empty_home).unwrap();
    let executable = std::fs::read_dir(&moved)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.is_file()
                && path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("portable-")
        })
        .unwrap();
    let library_name = HostCapability::Core.library_name(&dream_abi::target::TargetSpec::host());
    let library = moved.join(&library_name);
    inspect_loader(&executable, true);
    for capability in HostCapability::ALL {
        let path = moved.join(capability.library_name(&dream_abi::target::TargetSpec::host()));
        assert_eq!(
            path.is_file(),
            matches!(capability, HostCapability::Core | HostCapability::Unicode)
        );
        if path.is_file() {
            inspect_loader(&path, false);
        }
    }
    assert_runs_clean(&executable, &empty_home, temporary.path());

    // Failure without the shipped runtime proves the clean run did not find a toolchain copy.
    let hidden = moved.join("runtime unavailable");
    std::fs::rename(&library, &hidden).unwrap();
    let output = clean_output(&executable, &empty_home, temporary.path());
    std::fs::rename(hidden, library).unwrap();
    assert!(
        !output.status.success(),
        "executable found an unbundled runtime"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains(&library_name));
}

#[test]
fn loader_path_checks_reject_builder_and_working_directory_paths() {
    for path in [
        "/build/target/debug",
        "/home/builder/.dream/bin",
        "",
        ".",
        "../runtime",
        "$ORIGIN_suffix",
    ] {
        assert!(!package_relative(path), "{path}");
    }
    for path in [
        "$ORIGIN",
        "${ORIGIN}/lib",
        "@executable_path/../Frameworks",
        "@loader_path",
        "@rpath/libdream_host_core.dylib",
    ] {
        assert!(package_relative(path), "{path}");
    }
    assert_eq!(
        bracket_value("(RUNPATH) Library runpath: [/builder path:$ORIGIN]"),
        Some("/builder path:$ORIGIN")
    );
    assert_eq!(
        macos_rpaths(
            "cmd LC_RPATH\npath /builder path (offset 12)\ncmd LC_LOAD_DYLIB\npath /not/an/rpath (offset 24)"
        ),
        ["/builder path"]
    );
}
