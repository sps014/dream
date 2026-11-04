use crate::toolchain;
use anyhow::Result;

pub fn install(component: Option<String>) -> Result<()> {
    let tools = match component.as_deref() {
        None => toolchain::default_tools()?,
        Some(name) => vec![toolchain::parse_tool(name)?],
    };
    toolchain::install(&tools)?;
    prewarm_generators();
    prewarm_runtime();
    Ok(())
}

/// Compiles a tiny program natively and to wasm so the runtime objects and signature cache exist
/// before the first real build. Best effort, like [`prewarm_generators`].
fn prewarm_runtime() {
    let Ok(dream) = crate::dream_bin::locate() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("dream-prewarm-{}", std::process::id()));
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let file = dir.join("prewarm.dream");
    if std::fs::write(&file, "fun main(): void { }\n").is_err() {
        return;
    }
    for flag in ["--emit-llvm", "--wasm"] {
        let ok = std::process::Command::new(&dream)
            .arg(flag)
            .arg(&file)
            .status()
            .is_ok_and(|status| status.success());
        if !ok {
            eprintln!(
                "note: the {flag} runtime was not prebuilt; the first build of that kind builds it"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Builds the std generator executables so the first `@json` build only launches one. Best
/// effort: a toolchain that cannot build them yet (no LLVM) still installed fine.
fn prewarm_generators() {
    let Ok(dream) = crate::dream_bin::locate() else {
        return;
    };
    let ok = std::process::Command::new(&dream)
        .args(["generate", "--prewarm"])
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        eprintln!(
            "note: std generators were not prebuilt; the first build that uses one builds it"
        );
    }
}

pub fn list() -> Result<()> {
    toolchain::list()
}

pub fn uninstall(component: String) -> Result<()> {
    toolchain::uninstall(toolchain::parse_tool(&component)?)
}

pub fn doctor(target: Option<&str>, json: bool) -> Result<()> {
    let mut command = std::process::Command::new(crate::dream_bin::locate()?);
    command.arg("toolchain-doctor");
    if let Some(target) = target {
        command.args(["--target", target]);
    }
    if json {
        command.arg("--json");
    }
    if !command.status()?.success() {
        anyhow::bail!("toolchain diagnosis found missing requirements");
    }
    Ok(())
}
