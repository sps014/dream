//! Git dependencies on packages inside a multi-package repository (`subdir`), including a sibling
//! that the repository's own packages reference by relative path.

use dreamer::lockfile::Lockfile;
use dreamer::manifest::{Dependency, DetailedDependency, Manifest};
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn write_lib(dir: &Path, name: &str, deps: &[(&str, Dependency)]) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let mut manifest = Manifest::new_lib(name.into(), "0.1.0".into());
    for (dep, spec) in deps {
        manifest.dependencies.insert(dep.to_string(), spec.clone());
    }
    manifest.save(&dir.join("dream.toml")).unwrap();
    std::fs::write(
        dir.join("src").join(format!("{name}.dream")),
        "public fun answer(): int {\n    return 1;\n}\n",
    )
    .unwrap();
}

fn git_dep(url: &str, subdir: &str) -> Dependency {
    Dependency::Detailed(DetailedDependency {
        git: Some(url.into()),
        subdir: Some(subdir.into()),
        ..Default::default()
    })
}

#[test]
fn installs_sibling_packages_from_one_repository() {
    let tmp = tempfile::tempdir().unwrap();
    // SAFETY: this test binary has a single test, and nothing else reads the environment.
    unsafe { std::env::set_var("DREAM_HOME", tmp.path().join("home")) };

    let repo = tmp.path().join("packages");
    write_lib(&repo.join("json"), "json", &[]);
    write_lib(
        &repo.join("http"),
        "http",
        &[(
            "json",
            Dependency::Detailed(DetailedDependency {
                path: Some("../json".into()),
                ..Default::default()
            }),
        )],
    );
    git(&repo, &["init", "--quiet", "-b", "main"]);
    git(&repo, &["config", "user.email", "test@example.invalid"]);
    git(&repo, &["config", "user.name", "Test"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "packages"]);
    let url = repo.to_str().unwrap();

    let app = tmp.path().join("app");
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::write(app.join("src/main.dream"), "fun main(): void {}\n").unwrap();
    let mut manifest = Manifest::new("app".into(), "0.1.0".into(), "src/main.dream".into());
    manifest
        .dependencies
        .insert("http".into(), git_dep(url, "http"));
    manifest
        .dependencies
        .insert("json".into(), git_dep(url, "./json/"));
    manifest
        .dependencies
        .insert("stale".into(), git_dep(url, "json"));
    manifest.save(&app.join("dream.toml")).unwrap();

    // `stale` names the json directory under another package name and is rejected.
    assert!(dreamer::commands::install::run(&app).is_err());
    manifest.dependencies.remove("stale");
    manifest.save(&app.join("dream.toml")).unwrap();
    std::fs::create_dir_all(app.join("dream_packages/stale")).unwrap();

    dreamer::commands::install::run(&app).unwrap();
    let lock = Lockfile::load(&app.join("dream.lock")).unwrap();
    let http = lock.find("http").unwrap();
    let json = lock.find("json").unwrap();
    assert_eq!(http.git_subdir.as_deref(), Some("http"));
    assert_eq!(json.git_subdir.as_deref(), Some("json"));
    assert_eq!(http.source, json.source);
    assert!(app.join("dream_packages/http/src/http.dream").is_file());
    assert!(app.join("dream_packages/json/src/json.dream").is_file());
    assert!(!app.join("dream_packages/stale").exists());

    // A new upstream commit does not move the locked packages.
    std::fs::write(
        repo.join("json/src/json.dream"),
        "public fun answer(): int {\n    return 2;\n}\n",
    )
    .unwrap();
    git(&repo, &["commit", "--quiet", "-am", "next"]);
    dreamer::commands::install::run(&app).unwrap();
    let relocked = Lockfile::load(&app.join("dream.lock")).unwrap();
    assert_eq!(relocked.find("json").unwrap().source, json.source);
    let installed =
        std::fs::read_to_string(app.join("dream_packages/json/src/json.dream")).unwrap();
    assert!(installed.contains("return 1"), "{installed}");
}
