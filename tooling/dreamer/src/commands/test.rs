//! `dreamer test`: install (incl. dev-deps), then run test projects through `dream test`.
//!
//! At a workspace root every member that is a `type = "test"` package, or that has a `tests/`
//! directory, is a test project; elsewhere the enclosing package is (or `-p <name>`). Projects
//! run in parallel, each as its own `dream test` process, and one summary follows.

use crate::compile_flags::CompileFlags;
use crate::manifest::{Manifest, PackageType};
use crate::workspace::{self, Workspace};
use anyhow::{Result, bail};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

/// Host selection forwarded to `dream test` (none: each project's `[test].targets`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Hosts {
    pub native: bool,
    pub node: bool,
}

pub fn run(
    start_dir: &Path,
    release: bool,
    filter: Option<String>,
    package: Option<&str>,
) -> Result<()> {
    run_with(
        start_dir,
        CompileFlags {
            compile_profile: dream_abi::profile::CompileProfile::from_release(release),
            ..CompileFlags::default()
        },
        filter,
        package,
        Hosts::default(),
    )
}

struct Suite {
    name: String,
    dir: PathBuf,
}

pub fn run_with(
    start_dir: &Path,
    flags: CompileFlags,
    filter: Option<String>,
    package: Option<&str>,
    hosts: Hosts,
) -> Result<()> {
    super::install::run(start_dir)?;
    let suites = select_suites(start_dir, package)?;
    let dream_bin = crate::dream_bin::locate()?;
    let command = |suite: &Suite| {
        let mut cmd = Command::new(&dream_bin);
        flags.apply(&mut cmd);
        if hosts.node {
            cmd.arg("--node");
        }
        cmd.arg("test");
        if hosts.native {
            cmd.arg("--native");
        }
        if let Some(f) = &filter {
            cmd.arg("--filter").arg(f);
        }
        // Node cannot resolve Windows verbatim paths produced by workspace canonicalization.
        let directory = dunce::simplified(&suite.dir);
        cmd.arg(directory).current_dir(directory);
        cmd
    };

    if let [suite] = suites.as_slice() {
        let status = command(suite)
            .status()
            .map_err(|e| anyhow::anyhow!("running {}: {e}", dream_bin.display()))?;
        if !status.success() {
            bail!("tests failed in {}", suite.name);
        }
        return Ok(());
    }

    let outputs = run_parallel(&suites, command)?;
    let mut failed = Vec::new();
    for (suite, output) in suites.iter().zip(&outputs) {
        println!("── {} ({})", suite.name, suite.dir.display());
        print!("{}", String::from_utf8_lossy(&output.text));
        if !output.status.success() {
            failed.push(suite.name.as_str());
        }
    }
    let passed = suites.len() - failed.len();
    println!(
        "{} test project(s): {passed} passed, {} failed",
        suites.len(),
        failed.len()
    );
    if !failed.is_empty() {
        bail!("tests failed in {}", failed.join(", "));
    }
    Ok(())
}

struct SuiteOutput {
    status: ExitStatus,
    /// stdout and stderr through one pipe, so notes and test lines keep their order.
    text: Vec<u8>,
}

fn run_captured(mut cmd: Command) -> std::io::Result<SuiteOutput> {
    let (mut reader, writer) = std::io::pipe()?;
    cmd.stdin(Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer);
    let mut child = cmd.spawn()?;
    drop(cmd);
    let mut text = Vec::new();
    reader.read_to_end(&mut text)?;
    Ok(SuiteOutput {
        status: child.wait()?,
        text,
    })
}

/// Runs every suite's command, at most `available_parallelism` at a time, in suite order.
fn run_parallel(
    suites: &[Suite],
    command: impl Fn(&Suite) -> Command + Sync,
) -> Result<Vec<SuiteOutput>> {
    let width = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut outputs = Vec::with_capacity(suites.len());
    for chunk in suites.chunks(width) {
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|suite| {
                    let cmd = command(suite);
                    scope.spawn(move || run_captured(cmd))
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("test project thread"))
                .collect::<Vec<_>>()
        });
        for result in results {
            outputs.push(result.map_err(|e| anyhow::anyhow!("running dream test: {e}"))?);
        }
    }
    Ok(outputs)
}

fn select_suites(start_dir: &Path, package: Option<&str>) -> Result<Vec<Suite>> {
    if package.is_none()
        && let Some(ws_root) = Manifest::find_workspace_root(start_dir)
        && same_dir(&ws_root, start_dir)
    {
        let (_, members) = workspace::discover_install_root(start_dir)?;
        let suites: Vec<Suite> = members
            .iter()
            .filter_map(|(dir, manifest)| suite_for(dir, manifest))
            .collect();
        if suites.is_empty() {
            bail!(
                "no test projects in the workspace at {} (add a type = \"test\" member, or a \
                 tests/ directory to a member)",
                ws_root.display()
            );
        }
        return Ok(suites);
    }
    let ws = Workspace::discover_package(start_dir, package)?;
    match suite_for(&ws.root, &ws.manifest) {
        Some(suite) => Ok(vec![suite]),
        None => bail!(
            "package '{}' is not a type = \"test\" package and has no tests/ directory \
             (create tests/*.dream with @test functions, or `dreamer init --test`)",
            ws.manifest.package()?.name
        ),
    }
}

fn suite_for(dir: &Path, manifest: &Manifest) -> Option<Suite> {
    let pkg = manifest.package.as_ref()?;
    let is_test = pkg.package_type == PackageType::Test;
    (is_test || dir.join("tests").is_dir()).then(|| Suite {
        name: pkg.name.clone(),
        dir: dir.to_path_buf(),
    })
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    canon(a) == canon(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn workspace_root_selects_test_members_and_members_with_tests_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("dream.toml"),
            "[workspace]\nmembers = [\"app\", \"lib\", \"suite\"]\n",
        );
        write(
            &root.join("app/dream.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nentry = \"src/main.dream\"\n",
        );
        write(&root.join("app/tests/a.dream"), "");
        write(
            &root.join("lib/dream.toml"),
            "[package]\nname = \"lib\"\nversion = \"0.1.0\"\ntype = \"lib\"\n",
        );
        write(
            &root.join("suite/dream.toml"),
            "[package]\nname = \"suite\"\nversion = \"0.1.0\"\ntype = \"test\"\n",
        );
        let names: Vec<_> = select_suites(root, None)
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, vec!["app", "suite"]);
        let one: Vec<_> = select_suites(root, Some("suite"))
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(one, vec!["suite"]);
        assert!(select_suites(root, Some("lib")).is_err());
    }
}
