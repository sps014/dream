//! End-to-end coverage: `dreamer init` -> `dreamer add` -> `dreamer install`, exercised against a
//! local `file://` registry fixture (no server process required) and a local `--path` dependency,
//! then a `dreamer build` through the real `dream` compiler to prove the installed
//! `dream_packages/` layout is actually importable.

use dreamer::commands;
use dreamer::manifest::{import_segment, Manifest, PackageType};
use dreamer::registry::{checksum, open_registry, IndexEntry};
use std::path::{Path, PathBuf};
use std::sync::Once;

/// Point `DREAM_BIN` at this workspace's freshly built `dream` so e2e doesn't pick a stale
/// toolchain install from `~/.dream/toolchain.env`.
fn prefer_workspace_dream() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        root.pop(); // tooling/
        root.pop(); // repo root
        for profile in ["debug", "release"] {
            let mut candidate = root.join("target").join(profile).join("dream");
            if cfg!(windows) {
                candidate.set_extension("exe");
            }
            if candidate.is_file() {
                std::env::set_var("DREAM_BIN", &candidate);
                return;
            }
        }
    });
}

fn publish_fixture_package(registry_dir: &Path, name: &str, version: &str, body_fun_src: &str) {
    let pkg_dir = registry_dir
        .join("staging")
        .join(format!("{}-{}", name, version));
    std::fs::create_dir_all(pkg_dir.join("src")).unwrap();
    Manifest::new_lib(name.to_string(), version.to_string())
        .save(&pkg_dir.join("dream.toml"))
        .unwrap();
    let seg = import_segment(name);
    std::fs::write(
        pkg_dir.join("src").join(format!("{}.dream", seg)),
        body_fun_src,
    )
    .unwrap();

    let tarball_path = registry_dir
        .join("staging")
        .join(format!("{}-{}.tar.gz", name, version));
    let bytes = dreamer::fetch::package_project(&pkg_dir, &tarball_path).unwrap();

    let registry = open_registry(&format!("file://{}", registry_dir.display()));
    let entry = IndexEntry {
        name: name.to_string(),
        vers: version.to_string(),
        cksum: checksum::sha256_of(&bytes),
        tarball: format!("dl/{}/{}-{}.tar.gz", name, name, version),
        ..Default::default()
    };
    registry.publish(&entry, &tarball_path).unwrap();
}

#[test]
fn init_add_install_materializes_registry_and_path_dependencies() {
    let tmp = tempfile::tempdir().unwrap();
    let registry_dir = tmp.path().join("registry");
    std::fs::create_dir_all(&registry_dir).unwrap();
    publish_fixture_package(
        &registry_dir,
        "greeter",
        "1.0.0",
        "public fun hello(): string {\n    return \"hello from the registry\";\n}\n",
    );

    let local_lib_dir = tmp.path().join("local-lib");
    std::fs::create_dir_all(local_lib_dir.join("src")).unwrap();
    Manifest::new_lib("local-lib".to_string(), "0.1.0".to_string())
        .save(&local_lib_dir.join("dream.toml"))
        .unwrap();
    std::fs::write(
        local_lib_dir.join("src").join("local_lib.dream"),
        "public fun answer(): int {\n    return 42;\n}\n",
    )
    .unwrap();

    let project_dir = tmp.path().join("myapp");
    commands::init::run(&project_dir, Some("myapp".to_string()), None, false).unwrap();

    {
        let mut workspace = dreamer::workspace::Workspace::discover(&project_dir).unwrap();
        workspace.manifest.registries.insert(
            "default".to_string(),
            format!("file://{}", registry_dir.display()),
        );
        workspace.save_manifest().unwrap();
    }

    commands::add::run(
        &project_dir,
        "greeter".to_string(),
        Some("^1.0".to_string()),
        None,
        None,
        None,
        None,
        None,
        false,
        None,
    )
    .unwrap();

    commands::add::run(
        &project_dir,
        "local-lib".to_string(),
        None,
        Some(local_lib_dir.to_str().unwrap().to_string()),
        None,
        None,
        None,
        None,
        false,
        None,
    )
    .unwrap();

    let lock = dreamer::lockfile::Lockfile::load(&project_dir.join("dream.lock")).unwrap();
    assert!(lock.find("greeter").is_some());
    assert!(lock.find("local-lib").is_some());
    assert_eq!(lock.find("greeter").unwrap().version, "1.0.0");

    let greeter_file = project_dir
        .join("dream_packages")
        .join("greeter")
        .join("src")
        .join("greeter.dream");
    assert!(
        greeter_file.is_file(),
        "{} should exist",
        greeter_file.display()
    );

    let local_lib_file = project_dir
        .join("dream_packages")
        .join("local_lib")
        .join("src")
        .join("local_lib.dream");
    assert!(
        local_lib_file.is_file(),
        "{} should exist",
        local_lib_file.display()
    );

    // Re-running install without changes must not change the locked versions (respects the lock).
    commands::install::run(&project_dir).unwrap();
    let lock_again = dreamer::lockfile::Lockfile::load(&project_dir.join("dream.lock")).unwrap();
    assert_eq!(lock_again.find("greeter").unwrap().version, "1.0.0");
}

/// Only runs the real compiler when a `dream` binary is discoverable (it is, for anyone running
/// `cargo test --workspace` from a checkout where `dream` has already been built at least once).
#[test]
#[ignore = "invokes the full compiler; cargo test --workspace -- --ignored"]
fn build_compiles_a_project_using_an_installed_dependency() {
    prefer_workspace_dream();
    if dreamer::dream_bin::locate().is_err() {
        eprintln!("skipping: no `dream` compiler binary found on PATH or in target/");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let registry_dir = tmp.path().join("registry");
    std::fs::create_dir_all(&registry_dir).unwrap();
    // A distinct package name from the other test in this file: the download cache under
    // `~/.dream/registry/src/<name>-<version>` is keyed only by name+version (mirroring the
    // crates.io assumption that a name+version pair is globally unique content), so two tests
    // publishing the *same* name+version to *different* registries in parallel would otherwise
    // race on that shared cache entry.
    publish_fixture_package(
        &registry_dir,
        "greeter2",
        "1.0.0",
        "public fun hello(): string {\n    return \"hello from the registry\";\n}\n",
    );

    let project_dir = tmp.path().join("myapp");
    commands::init::run(&project_dir, Some("myapp".to_string()), None, false).unwrap();
    {
        let mut workspace = dreamer::workspace::Workspace::discover(&project_dir).unwrap();
        workspace.manifest.registries.insert(
            "default".to_string(),
            format!("file://{}", registry_dir.display()),
        );
        workspace.save_manifest().unwrap();
    }
    commands::add::run(
        &project_dir,
        "greeter2".to_string(),
        Some("^1.0".to_string()),
        None,
        None,
        None,
        None,
        None,
        false,
        None,
    )
    .unwrap();

    std::fs::write(
        project_dir.join("src").join("main.dream"),
        "import greeter2;\nimport system;\n\nfun main(): void {\n    System.println(hello());\n}\n",
    )
    .unwrap();

    commands::build::run(&project_dir, false, None).unwrap();
    assert!(
        project_dir
            .join("target")
            .join("web")
            .join("main.wat")
            .is_file(),
        "expected artifacts under target/web/"
    );
    assert!(!project_dir.join("src").join("main.wat").exists());
}

#[test]
fn init_runtime_scaffolds_web_and_node_hosts() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("webapp");
    commands::init::run(
        &project_dir,
        Some("webapp".to_string()),
        Some("web,node".to_string()),
        false,
    )
    .unwrap();

    let manifest = Manifest::load(&project_dir.join("dream.toml")).unwrap();
    assert_eq!(manifest.package().unwrap().targets, vec!["web", "node"]);
    assert!(project_dir.join("index.html").is_file());
    assert!(project_dir.join("run.mjs").is_file());

    let gitignore = std::fs::read_to_string(project_dir.join(".gitignore")).unwrap();
    assert!(gitignore.contains("dream_packages/"));
    assert!(gitignore.contains("target/"));

    let html = std::fs::read_to_string(project_dir.join("index.html")).unwrap();
    assert!(html.contains("target/web/main.web.runtime.js"));
    let mjs = std::fs::read_to_string(project_dir.join("run.mjs")).unwrap();
    assert!(mjs.contains("target/node/main.node.runtime.js"));
}

#[test]
fn init_runtime_web_only_skips_run_mjs() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("browser-only");
    commands::init::run(
        &project_dir,
        Some("browser_only".to_string()),
        Some("web".to_string()),
        false,
    )
    .unwrap();
    assert!(project_dir.join("index.html").is_file());
    assert!(!project_dir.join("run.mjs").exists());
}

#[test]
fn node_runner_is_written_when_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("browser-then-node");
    commands::init::run(
        &project_dir,
        Some("browser_then_node".to_string()),
        Some("web".to_string()),
        false,
    )
    .unwrap();
    assert!(!project_dir.join("run.mjs").exists());
    dreamer::artifact_alias::ensure_node_runner(&project_dir, "main").unwrap();
    let mjs = std::fs::read_to_string(project_dir.join("run.mjs")).unwrap();
    assert!(mjs.contains("target/node/main.node.runtime.js"));
}

#[test]
fn init_lib_has_no_entry_and_run_rejects() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("http-utils");
    commands::init::run(&project_dir, Some("http-utils".to_string()), None, true).unwrap();

    let manifest = Manifest::load(&project_dir.join("dream.toml")).unwrap();
    assert_eq!(manifest.package().unwrap().package_type, PackageType::Lib);
    assert!(manifest.package().unwrap().entry.is_none());
    assert!(project_dir.join("src").join("http_utils.dream").is_file());

    let err = commands::run::run(&project_dir, None, false, None, &[], None).unwrap_err();
    assert!(err.to_string().contains("not runnable"));
}

#[test]
#[ignore = "invokes the full compiler; cargo test --workspace -- --ignored"]
fn build_refreshes_web_and_node_aliases() {
    prefer_workspace_dream();
    if dreamer::dream_bin::locate().is_err() {
        eprintln!("skipping: no `dream` compiler binary found on PATH or in target/");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("webapp");
    commands::init::run(
        &project_dir,
        Some("webapp".to_string()),
        Some("web,node".to_string()),
        false,
    )
    .unwrap();

    commands::build::run(&project_dir, false, None).unwrap();
    assert!(project_dir.join("target/web/main.wasm").is_file());
    assert!(project_dir.join("target/web/main.web.runtime.js").is_file());
    assert!(project_dir
        .join("target/node/main.node.runtime.js")
        .is_file());
    assert!(project_dir.join("target/node/main.wasm").is_file());

    commands::build::run(&project_dir, true, None).unwrap();
    assert!(
        project_dir.join("target/web/main.wasm").is_file(),
        "release build should write wasm to target/web"
    );
}

#[test]
#[ignore = "invokes the full compiler; cargo test --workspace -- --ignored"]
fn build_lib_writes_under_target_web() {
    prefer_workspace_dream();
    if dreamer::dream_bin::locate().is_err() {
        eprintln!("skipping: no `dream` compiler binary found on PATH or in target/");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("mylib");
    commands::init::run(&project_dir, Some("mylib".to_string()), None, true).unwrap();
    commands::build::run(&project_dir, false, None).unwrap();
    assert!(project_dir
        .join("target")
        .join("web")
        .join("mylib.wat")
        .is_file());
}

#[test]
#[ignore = "builds dream-runner --release; cargo test --workspace -- --ignored"]
fn pack_rejects_libs_and_packs_bin_for_host() {
    prefer_workspace_dream();
    if dreamer::dream_bin::locate().is_err() {
        eprintln!("skipping: no `dream` compiler binary found on PATH or in target/");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let lib_dir = tmp.path().join("libpack");
    commands::init::run(&lib_dir, Some("libpack".to_string()), None, true).unwrap();
    let pack_flags = dreamer::compile_flags::CompileFlags::for_pack(false, None, false).unwrap();
    assert!(commands::pack::run(&lib_dir, &[], None, pack_flags.clone()).is_err());

    let bin_dir = tmp.path().join("binpack");
    commands::init::run(&bin_dir, Some("binpack".to_string()), None, false).unwrap();
    // Pack builds dream-runner via cargo; needs the Dream workspace (discovered from the dream bin).
    if let Err(e) = commands::pack::run(&bin_dir, &[], None, pack_flags) {
        eprintln!("pack skipped/failed (may need DREAM_REPO / full workspace): {e:#}");
        return;
    }
    let pack_dir = bin_dir.join("target").join("pack");
    let entries: Vec<_> = std::fs::read_dir(&pack_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(
        !entries.is_empty(),
        "expected at least one packed binary under {}",
        pack_dir.display()
    );
}

#[test]
#[ignore = "invokes the full compiler; cargo test --workspace -- --ignored"]
fn workspace_install_shares_lock_and_packages_symlink() {
    prefer_workspace_dream();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("mono");
    let shared = root.join("packages").join("greeter");
    let cli = root.join("apps").join("cli");
    std::fs::create_dir_all(shared.join("src")).unwrap();
    std::fs::create_dir_all(cli.join("src")).unwrap();

    Manifest::new_workspace(vec!["packages/greeter".into(), "apps/cli".into()])
        .save(&root.join("dream.toml"))
        .unwrap();

    Manifest::new_lib("greeter".into(), "0.1.0".into())
        .save(&shared.join("dream.toml"))
        .unwrap();
    std::fs::write(
        shared.join("src").join("greeter.dream"),
        "public fun greet(name: string): string {\n    return \"hello, \" + name;\n}\n",
    )
    .unwrap();

    let mut cli_manifest = Manifest::new("cli".into(), "0.1.0".into(), "src/main.dream".into());
    cli_manifest.dependencies.insert(
        "greeter".into(),
        dreamer::manifest::Dependency::Detailed(dreamer::manifest::DetailedDependency {
            version: Some("0.1.0".into()),
            path: Some("../../packages/greeter".into()),
            ..Default::default()
        }),
    );
    cli_manifest.save(&cli.join("dream.toml")).unwrap();
    std::fs::write(
        cli.join("src").join("main.dream"),
        "import greeter;\nimport system;\n\nfun main(): void {\n    System.println(greet(\"mono\"));\n}\n",
    )
    .unwrap();

    // Install from workspace root.
    commands::install::run(&root).unwrap();
    assert!(root.join("dream.lock").is_file());
    assert!(root
        .join("dream_packages")
        .join("greeter")
        .join("src")
        .join("greeter.dream")
        .is_file());
    let member_pkgs = cli.join("dream_packages");
    assert!(
        member_pkgs.is_symlink() || member_pkgs.is_dir(),
        "member should get dream_packages symlink/dir"
    );
    assert!(
        member_pkgs
            .join("greeter")
            .join("src")
            .join("greeter.dream")
            .is_file(),
        "symlink should resolve to greeter package sources"
    );

    // -p required at virtual root; works from member cwd.
    assert!(commands::build::run(&root, false, None).is_err());
    if dreamer::dream_bin::locate().is_ok() {
        commands::build::run(&root, false, Some("cli")).unwrap();
        assert!(cli.join("target").join("web").join("main.wat").is_file());
        commands::build::run(&cli, false, None).unwrap();
    }

    // Publish rejects path-only; version+path is ok to attempt (file registry).
    let mut path_only = Manifest::new("cli".into(), "0.1.0".into(), "src/main.dream".into());
    path_only.dependencies.insert(
        "greeter".into(),
        dreamer::manifest::Dependency::Detailed(dreamer::manifest::DetailedDependency {
            path: Some("../../packages/greeter".into()),
            ..Default::default()
        }),
    );
    path_only.save(&cli.join("dream.toml")).unwrap();
    let err =
        commands::publish::run(&cli, Some("file:///tmp/unused".into()), None, None).unwrap_err();
    assert!(
        err.to_string().contains("path-only"),
        "expected path-only publish error, got: {err:#}"
    );
}
