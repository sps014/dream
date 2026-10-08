use super::*;
use std::collections::BTreeMap;

fn config(values: &[(&str, &str)], exe: Option<PathBuf>, cwd: PathBuf) -> ToolchainConfig {
    let vars: BTreeMap<_, _> = values.iter().copied().collect();
    ToolchainConfig::from_lookup(|key| vars.get(key).map(OsString::from), exe, cwd)
}

#[test]
fn runtime_instrumentation_has_a_distinct_cache_identity() {
    let ordinary = config(&[], None, PathBuf::from("/project"));
    let measured = config(
        &[("DREAM_RUNTIME_COUNTERS", "1")],
        None,
        PathBuf::from("/project"),
    );
    assert!(!ordinary.runtime_counters);
    assert!(measured.runtime_counters);
    assert_ne!(ordinary.fingerprint(), measured.fingerprint());
}

#[test]
fn explicit_paths_and_compiler_precedence_are_captured() {
    let c = config(
        &[
            ("DREAM_HOME", "/install/bin"),
            ("DREAM_BIN", "/other/dream"),
            ("DREAM_TOOLCHAINS", "/tools"),
            ("DREAM_LLVM", "/llvm/bin"),
            ("DREAM_RUNTIME_C", "/runtime"),
            ("DREAM_CC", "dream-clang"),
            ("CC", "other-clang"),
            ("DREAM_CXX", "dream-clang++"),
            ("CXX", "other-clang++"),
        ],
        None,
        PathBuf::from("/project"),
    );
    assert_eq!(c.prefix, PathBuf::from("/install"));
    assert_eq!(
        c.toolchains,
        vec![
            PathBuf::from("/tools"),
            PathBuf::from("/install/toolchains")
        ]
    );
    assert_eq!(c.llvm, Some(PathBuf::from("/llvm/bin")));
    assert_eq!(c.runtime_c, PathBuf::from("/runtime"));
    assert_eq!(c.cc, Some(OsString::from("dream-clang")));
    assert_eq!(c.cxx, Some(OsString::from("dream-clang++")));
}

#[test]
fn empty_overrides_use_user_defaults_not_cargo_output_as_install_prefix() {
    let c = config(
        &[
            ("DREAM_HOME", "/project/target/debug"),
            ("DREAM_TOOLCHAINS", ""),
            ("DREAM_LLVM", ""),
            ("HOME", "/user"),
            ("DREAM_CC", ""),
            ("CC", "clang"),
        ],
        None,
        PathBuf::from("/project"),
    );
    assert_eq!(c.prefix, PathBuf::from("/user/.dream"));
    assert_eq!(c.toolchains, vec![PathBuf::from("/user/.dream/toolchains")]);
    assert!(c.llvm.is_none());
    assert_eq!(c.cc, Some(OsString::from("clang")));
}

#[test]
fn hosts_never_search_the_working_directory() {
    let root = tempfile::tempdir().unwrap();
    let c = config(
        &[],
        Some(root.path().join("compiler/deps/dream")),
        root.path().join("project"),
    );
    let dirs = c.host_library_dirs();
    assert_eq!(
        dirs,
        vec![
            root.path().join("compiler/deps"),
            root.path().join("compiler")
        ]
    );
    assert!(!dirs.contains(&c.cwd));
}

#[test]
fn captured_path_is_used_for_program_resolution() {
    let root = tempfile::tempdir().unwrap();
    let name = if cfg!(windows) {
        "dream-test-cc.exe"
    } else {
        "dream-test-cc"
    };
    std::fs::write(root.path().join(name), []).unwrap();
    let path = std::env::join_paths([root.path()]).unwrap();
    let c = ToolchainConfig::from_lookup(
        |key| (key == "PATH").then(|| path.clone()),
        None,
        root.path().to_path_buf(),
    );
    assert_eq!(
        c.find_on_path("dream-test-cc"),
        Some(root.path().join(name))
    );
}

#[test]
fn configs_are_independent_without_mutating_process_environment() {
    let a = config(&[("DREAM_RUNTIME_C", "first")], None, PathBuf::from("."));
    let b = config(&[("DREAM_RUNTIME_C", "second")], None, PathBuf::from("."));
    assert_eq!(a.runtime_c, PathBuf::from("first"));
    assert_eq!(b.runtime_c, PathBuf::from("second"));
}

#[test]
fn cache_locations_do_not_depend_on_cargo_files_in_the_working_directory() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir_all(project.join("target")).unwrap();
    std::fs::write(project.join("Cargo.toml"), "").unwrap();
    let prefix = root.path().join("install");
    let a = ToolchainConfig::from_lookup(
        |key| (key == "DREAM_PREFIX").then(|| prefix.clone().into_os_string()),
        None,
        project,
    );
    let b = ToolchainConfig::from_lookup(
        |key| (key == "DREAM_PREFIX").then(|| prefix.clone().into_os_string()),
        None,
        root.path().join("elsewhere"),
    );
    assert_eq!(a.native_rt_cache_root(), prefix.join("cache/native-rt"));
    assert_eq!(a.generator_cache_root(), prefix.join("cache/generators"));
    assert_eq!(a.native_rt_cache_root(), b.native_rt_cache_root());
    assert_eq!(a.generator_cache_root(), b.generator_cache_root());
}

#[test]
fn execution_and_runtime_catalog_do_not_read_environment_again() {
    for source in [
        include_str!("../../execution/llvm/tools.rs"),
        include_str!("../../execution/llvm/wasm_opt.rs"),
        include_str!("../../execution/llvm/runtime.rs"),
        include_str!("../../execution/llvm/bundle.rs"),
        include_str!("../../execution/native/cc.rs"),
        include_str!("../../execution/native/c_link.rs"),
        include_str!("../../execution/native/mod.rs"),
        include_str!("../../execution/debugger/mod.rs"),
        include_str!("../../../crates/dream-mir/src/runtime/modules.rs"),
    ] {
        assert!(!source.contains("std::env::var"));
    }
}

#[test]
fn target_sysroot_and_driver_changes_invalidate_configuration_hash() {
    let base = config(&[], None, PathBuf::from("/project"));
    assert_eq!(base.targets, base.prefix.join("targets"));
    for values in [
        [("DREAM_TARGETS", "/foreign")],
        [("DREAM_SYSROOT", "/sdk")],
        [("DREAM_CC", "other-clang")],
        [("DREAM_WASM_OPT", "/binaryen/bin/wasm-opt")],
        [("CPATH", "/extra-headers")],
        [("LIBRARY_PATH", "/extra-libraries")],
    ] {
        let changed = config(&values, None, PathBuf::from("/project"));
        assert_ne!(base.fingerprint(), changed.fingerprint());
        assert_eq!(
            changed.fingerprint(),
            config(&values, None, PathBuf::from("/project")).fingerprint()
        );
    }
}
