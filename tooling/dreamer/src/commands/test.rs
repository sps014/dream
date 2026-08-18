//! `dreamer test`: install (incl. dev-deps), then invoke `dream test` on the project's `tests/`.

use crate::compile_flags::CompileFlags;
use crate::workspace::Workspace;
use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

pub fn run(
    start_dir: &Path,
    release: bool,
    filter: Option<String>,
    package: Option<&str>,
) -> Result<()> {
    run_with(
        start_dir,
        CompileFlags {
            release,
            ..CompileFlags::default()
        },
        filter,
        package,
    )
}

pub fn run_with(
    start_dir: &Path,
    flags: CompileFlags,
    filter: Option<String>,
    package: Option<&str>,
) -> Result<()> {
    super::install::run(start_dir)?;
    let workspace = Workspace::discover_package(start_dir, package)?;
    let tests_dir = workspace.root.join("tests");
    if !tests_dir.is_dir() {
        bail!(
            "no tests/ directory in {} (create tests/*.dream with @test functions)",
            workspace.root.display()
        );
    }

    let dream_bin = crate::dream_bin::locate()?;
    let mut cmd = Command::new(&dream_bin);
    flags.apply(&mut cmd);
    cmd.arg("test");
    if let Some(f) = filter {
        cmd.arg("--filter").arg(f);
    }
    cmd.arg(&tests_dir);
    cmd.current_dir(&workspace.root);

    let status = cmd
        .status()
        .map_err(|e| anyhow::anyhow!("running {}: {}", dream_bin.display(), e))?;
    if !status.success() {
        bail!("tests failed (exit code {:?})", status.code());
    }
    Ok(())
}
