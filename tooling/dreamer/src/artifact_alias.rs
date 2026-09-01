//! Stable host artifact dirs under `target/web/` and `target/node/`.
//!
//! The compiler writes wasm to `target/web/` (debug and `--release` share that path). After a
//! build that emitted a Node runtime, dreamer copies the wasm + `*.node.runtime.js` into
//! `target/node/` so `run.mjs` can use a host-specific folder.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

/// Ensure `target/web/` has the wasm (+ web runtime when requested) and refresh `target/node/`
/// from that same folder when Node was part of the compile.
pub fn refresh_host_aliases(
    project_root: &Path,
    entry_stem: &str,
    web: bool,
    node: bool,
) -> Result<()> {
    if !web && !node {
        return Ok(());
    }

    let web_dir = project_root.join("target").join("web");
    if !web_dir.is_dir() {
        bail!(
            "expected compile artifacts under {} after build",
            web_dir.display()
        );
    }

    copy_required(&web_dir, &web_dir, &format!("{entry_stem}.wasm"))?;
    if web {
        copy_required(
            &web_dir,
            &web_dir,
            &format!("{entry_stem}.web.runtime.js"),
        )?;
    }
    let _ = copy_if_present(&web_dir, &web_dir, &format!("{entry_stem}.abi.json"));

    if node {
        refresh_node(&web_dir, &project_root.join("target").join("node"), entry_stem)?;
    }
    Ok(())
}

fn refresh_node(src_dir: &Path, dest_dir: &Path, stem: &str) -> Result<()> {
    fs::create_dir_all(dest_dir)
        .with_context(|| format!("creating alias dir {}", dest_dir.display()))?;
    copy_required(src_dir, dest_dir, &format!("{stem}.wasm"))?;
    copy_required(src_dir, dest_dir, &format!("{stem}.node.runtime.js"))?;
    let _ = copy_if_present(src_dir, dest_dir, &format!("{stem}.abi.json"));
    Ok(())
}

fn copy_required(src_dir: &Path, dest_dir: &Path, name: &str) -> Result<()> {
    let src = src_dir.join(name);
    if !src.is_file() {
        bail!(
            "missing {} after build (needed for host alias {})",
            src.display(),
            dest_dir.display()
        );
    }
    if src_dir == dest_dir {
        return Ok(());
    }
    let dest = dest_dir.join(name);
    fs::copy(&src, &dest)
        .with_context(|| format!("copying {} → {}", src.display(), dest.display()))?;
    Ok(())
}

fn copy_if_present(src_dir: &Path, dest_dir: &Path, name: &str) -> Result<bool> {
    let src = src_dir.join(name);
    if !src.is_file() {
        return Ok(false);
    }
    if src_dir == dest_dir {
        return Ok(true);
    }
    let dest = dest_dir.join(name);
    fs::copy(&src, &dest)
        .with_context(|| format!("copying {} → {}", src.display(), dest.display()))?;
    Ok(true)
}

/// Contents of `run.mjs` for a given compile-root stem (`main` → `target/node/main.wasm`).
pub fn node_runner_source(stem: &str) -> String {
    format!(
        "import {{ run }} from \"./target/node/{stem}.node.runtime.js\";\nawait run(\"./target/node/{stem}.wasm\");\n"
    )
}

/// Write `run.mjs` when missing so `dreamer run --target node` works after `"node"` is added to
/// `package.targets` without re-running `dreamer init --runtime node`.
pub fn ensure_node_runner(project_root: &Path, stem: &str) -> Result<()> {
    let path = project_root.join("run.mjs");
    if path.is_file() {
        return Ok(());
    }
    fs::write(&path, node_runner_source(stem))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// File stem of the package compile root (`src/main.dream` → `main`).
pub fn entry_stem(compile_root: &Path) -> Result<String> {
    compile_root
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("compile root has no file stem: {}", compile_root.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_copies_node_from_web() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let web = root.join("target").join("web");
        fs::create_dir_all(&web).unwrap();
        fs::write(web.join("main.wasm"), b"wasm").unwrap();
        fs::write(web.join("main.web.runtime.js"), b"web").unwrap();
        fs::write(web.join("main.node.runtime.js"), b"node").unwrap();
        fs::write(web.join("main.abi.json"), b"{}").unwrap();

        refresh_host_aliases(root, "main", true, true).unwrap();

        assert_eq!(fs::read(root.join("target/web/main.wasm")).unwrap(), b"wasm");
        assert_eq!(
            fs::read(root.join("target/node/main.node.runtime.js")).unwrap(),
            b"node"
        );
        assert_eq!(fs::read(root.join("target/node/main.wasm")).unwrap(), b"wasm");
    }

    #[test]
    fn refresh_web_only_stays_in_web() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let web = root.join("target").join("web");
        fs::create_dir_all(&web).unwrap();
        fs::write(web.join("app.wasm"), b"r").unwrap();
        fs::write(web.join("app.web.runtime.js"), b"w").unwrap();

        refresh_host_aliases(root, "app", true, false).unwrap();
        assert!(root.join("target/web/app.wasm").is_file());
        assert!(!root.join("target/node").exists());
    }

    #[test]
    fn ensure_node_runner_writes_stem_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        ensure_node_runner(root, "app").unwrap();
        let body = fs::read_to_string(root.join("run.mjs")).unwrap();
        assert!(body.contains("target/node/app.node.runtime.js"));
        assert!(body.contains("target/node/app.wasm"));
        ensure_node_runner(root, "other").unwrap();
        let again = fs::read_to_string(root.join("run.mjs")).unwrap();
        assert_eq!(body, again);
    }
}
