//! Pinned Binaryen executable, installed by Dreamer rather than built into the compiler.

use crate::driver::{toolchain::ToolchainConfig, wasm_opt::OptLevel};
use dream_abi::toolchain::BINARYEN_VERSION;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

const MISSING: &str =
    "Binaryen not found; run `dreamer toolchain install binaryen` or set DREAM_WASM_OPT";
const FEATURES: &[&str] = &[
    "--mvp-features",
    "--enable-mutable-globals",
    "--enable-sign-ext",
    "--enable-nontrapping-float-to-int",
    "--enable-bulk-memory",
    "--enable-reference-types",
    "--enable-multivalue",
    "--enable-simd",
    "--enable-multimemory",
    "--enable-relaxed-simd",
    "--enable-tail-call",
    "--enable-extended-const",
    "--enable-threads",
    "--enable-exception-handling",
];

pub fn optimize(path: &Path, level: OptLevel, config: &Arc<ToolchainConfig>) -> Result<(), String> {
    let binary = resolve(config)?;
    let mut command = Command::new(binary);
    command
        .arg(path)
        .arg("-o")
        .arg(path)
        .arg(level.as_cli_flag())
        .arg("--converge")
        .args(FEATURES);
    if matches!(level, OptLevel::Size | OptLevel::SizeAggressive) {
        command.args(["--strip-debug", "--strip-producers"]);
    }
    let output = command
        .output()
        .map_err(|e| format!("running wasm-opt: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "wasm-opt failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

pub fn resolve_existing(config: &ToolchainConfig) -> Option<PathBuf> {
    if let Some(path) = &config.wasm_opt {
        return Some(path.clone());
    }
    let binary = if cfg!(windows) {
        "wasm-opt.exe"
    } else {
        "wasm-opt"
    };
    config
        .toolchains
        .iter()
        .map(|root| {
            root.join(format!("binaryen-{BINARYEN_VERSION}"))
                .join("bin")
                .join(binary)
        })
        .find(|path| path.is_file())
}

pub fn resolve(config: &Arc<ToolchainConfig>) -> Result<PathBuf, String> {
    config.resolved_wasm_opt.get_or_init(|| {
        if let Some(path) = resolve_existing(config) { return validate(path); }
        if config.no_auto_install { return Err(MISSING.into()); }
        // Separate compiler configurations can race to install the same user toolchain.
        static INSTALL: Mutex<()> = Mutex::new(());
        let _guard = INSTALL.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(path) = resolve_existing(config) { return validate(path); }
        let name = if cfg!(windows) { "dreamer.exe" } else { "dreamer" };
        let dreamer = config.host_library_dirs().into_iter().map(|dir| dir.join(name))
            .find(|path| path.is_file()).or_else(|| config.find_on_path("dreamer"))
            .ok_or_else(|| MISSING.to_string())?;
        eprintln!("Installing Binaryen {BINARYEN_VERSION} for WASM optimization (DREAM_NO_AUTO_INSTALL=1 disables downloads)");
        let mut command = Command::new(&dreamer);
        command.args(["toolchain", "install", "binaryen"]).env("DREAM_PREFIX", &config.prefix)
            .stdout(Stdio::from(std::io::stderr()));
        if let Some(root) = config.toolchains.first() { command.env("DREAM_TOOLCHAINS", root); }
        let status = command.status().map_err(|e|format!("running {}: {e}", dreamer.display()))?;
        if !status.success() { return Err(format!("`dreamer toolchain install binaryen` failed; {MISSING}")); }
        validate(resolve_existing(config).ok_or_else(|| MISSING.to_string())?)
    }).clone()
}

pub(super) fn validate(path: PathBuf) -> Result<PathBuf, String> {
    let output = Command::new(&path)
        .arg("--version")
        .output()
        .map_err(|e| format!("running {}: {e}", path.display()))?;
    let version = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || version.split_whitespace().nth(2) != Some(BINARYEN_VERSION) {
        return Err(format!(
            "wasm-opt at {} reports `{}`; Dream requires Binaryen {BINARYEN_VERSION}",
            path.display(),
            version.trim()
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(root: &Path) -> ToolchainConfig {
        ToolchainConfig {
            wasm_opt: None,
            toolchains: vec![root.join("toolchains")],
            prefix: root.to_path_buf(),
            path: Vec::new(),
            no_auto_install: true,
            ..ToolchainConfig::default()
        }
    }
    #[test]
    fn missing_optimizer_respects_offline_configuration() {
        let temp = tempfile::tempdir().unwrap();
        assert!(resolve(&Arc::new(config(temp.path())))
            .unwrap_err()
            .contains("Binaryen not found"));
    }
    #[cfg(unix)]
    fn executable(path: &Path, source: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, source).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn rejects_wrong_version_without_installing_over_override() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("wasm-opt");
        executable(&binary, "#!/bin/sh\necho 'wasm-opt version 116'\n");
        let mut config = config(temp.path());
        config.wasm_opt = Some(binary);
        assert!(resolve(&Arc::new(config))
            .unwrap_err()
            .contains("requires Binaryen 133"));
    }
    #[test]
    #[cfg(unix)]
    fn optimization_failure_is_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("wasm-opt");
        executable(&binary, &format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'wasm-opt version {BINARYEN_VERSION}'; else echo 'invalid wasm input' >&2; exit 2; fi\n"));
        let mut configuration = config(temp.path());
        configuration.wasm_opt = Some(binary);
        let wasm = temp.path().join("main.wasm");
        std::fs::write(&wasm, b"invalid").unwrap();
        let error = optimize(&wasm, OptLevel::O3, &Arc::new(configuration)).unwrap_err();
        assert!(error.contains("invalid wasm input"));
        assert_eq!(std::fs::read(wasm).unwrap(), b"invalid");
    }

    #[test]
    #[cfg(unix)]
    fn first_use_installs_once_and_preserves_snapshot_paths() {
        let temp = tempfile::tempdir().unwrap();
        let dreamer = temp.path().join("dreamer");
        executable(&dreamer, &format!("#!/bin/sh\n[ \"$*\" = 'toolchain install binaryen' ] || exit 1\nmkdir -p \"$DREAM_TOOLCHAINS/binaryen-{BINARYEN_VERSION}/bin\"\nprintf '#!/bin/sh\\necho wasm-opt version {BINARYEN_VERSION}\\n' > \"$DREAM_TOOLCHAINS/binaryen-{BINARYEN_VERSION}/bin/wasm-opt\"\nchmod +x \"$DREAM_TOOLCHAINS/binaryen-{BINARYEN_VERSION}/bin/wasm-opt\"\necho installed >> \"$DREAM_PREFIX/install-count\"\n"));
        let mut config = config(temp.path());
        config.exe = Some(temp.path().join("dream"));
        config.no_auto_install = false;
        let config = Arc::new(config);
        assert_eq!(resolve(&config).unwrap(), resolve(&config).unwrap());
        assert_eq!(
            std::fs::read_to_string(temp.path().join("install-count")).unwrap(),
            "installed\n"
        );
    }
}
