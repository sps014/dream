//! Locates the `dream` compiler executable that `dreamer build`/`dreamer run` shell out to.
//!
//! Resolution order (first hit wins):
//! 1. `DREAM_BIN` — exact path to the `dream` binary
//! 2. `DREAM_HOME` — directory containing `dream` / `dream.exe`
//! 3. `~/.dream/toolchain.env` written by `use-toolchain.sh` (for tools run outside that shell)
//! 4. `dream` on `PATH`
//! 5. Sibling of this `dreamer` executable (same Cargo `target/{debug,release}/`)

use anyhow::{Result, bail};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn read_toolchain_env_file() -> BTreeMap<String, String> {
    let home_key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let home = std::env::var_os(home_key)
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::home_dir);
    let Some(home) = home else {
        return BTreeMap::new();
    };
    let path = home.join(".dream").join("toolchain.env");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let mut value = value.trim().to_string();
        if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            value = value[1..value.len() - 1].to_string();
        }
        out.insert(key.trim().to_string(), value);
    }
    out
}

fn env_or_toolchain(key: &str, file: &BTreeMap<String, String>) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| file.get(key).cloned())
}

pub fn locate() -> Result<PathBuf> {
    let file = read_toolchain_env_file();

    if let Some(custom) = env_or_toolchain("DREAM_BIN", &file) {
        let path = PathBuf::from(&custom);
        if path.is_file() {
            return Ok(path);
        }
        if std::env::var_os("DREAM_BIN").is_some() {
            bail!(
                "DREAM_BIN is set to '{}' but that path is not a file",
                path.display()
            );
        }
    }

    if let Some(home) = env_or_toolchain("DREAM_HOME", &file) {
        let home = PathBuf::from(&home);
        if let Some(path) = binary_in_dir(&home, "dream") {
            return Ok(path);
        }
        if std::env::var_os("DREAM_HOME").is_some() {
            bail!(
                "DREAM_HOME is set to '{}' but no `dream` binary was found there",
                home.display()
            );
        }
    }

    if let Some(on_path) = find_on_path("dream") {
        return Ok(on_path);
    }

    // When `dreamer` itself was built inside the Dream workspace, its own binary and the `dream`
    // compiler binary land as siblings in the same `target/{debug,release}/` directory (one
    // shared target dir per Cargo workspace), independent of the project being built.
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
        && let Some(sibling) = binary_in_dir(dir, "dream")
    {
        return Ok(sibling);
    }

    bail!(
        "could not find the `dream` compiler executable; install it on PATH, set DREAM_HOME \
         (directory containing `dream`) or DREAM_BIN (exact path), run `source ./use-toolchain.sh` \
         (writes ~/.dream/toolchain.env), or place it beside `dreamer`"
    )
}

/// Locates the `dreamer` package-manager binary (for tooling that shells out to it).
///
/// Resolution order: `DREAMER_HOME` → `~/.dream/toolchain.env` → `PATH` → sibling of
/// `current_exe`.
pub fn locate_dreamer() -> Result<PathBuf> {
    let file = read_toolchain_env_file();

    if let Some(home) =
        env_or_toolchain("DREAMER_HOME", &file).or_else(|| env_or_toolchain("DREAM_HOME", &file))
    {
        let home = PathBuf::from(&home);
        if let Some(path) = binary_in_dir(&home, "dreamer") {
            return Ok(path);
        }
        if std::env::var_os("DREAMER_HOME").is_some() {
            bail!(
                "DREAMER_HOME is set to '{}' but no `dreamer` binary was found there",
                home.display()
            );
        }
    }

    if let Some(on_path) = find_on_path("dreamer") {
        return Ok(on_path);
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
        && let Some(sibling) = binary_in_dir(dir, "dreamer")
    {
        return Ok(sibling);
    }

    bail!(
        "could not find the `dreamer` executable; install it on PATH, set DREAMER_HOME \
         (directory containing `dreamer`), run `source ./use-toolchain.sh`, or \
         place it beside the calling executable"
    )
}

fn binary_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        dir.join(format!("{}.exe", name))
    } else {
        dir.join(name)
    };
    exe.is_file().then_some(exe)
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let exe_name = if cfg!(windows) {
        format!("{}.exe", name)
    } else {
        name.to_string()
    };
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(&exe_name))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env mutation must be serialized across tests in this process.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn binary_in_dir_finds_named_file() {
        let tmp = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) { "dream.exe" } else { "dream" };
        let path = tmp.path().join(name);
        std::fs::write(&path, b"").unwrap();
        assert_eq!(
            binary_in_dir(tmp.path(), "dream").as_deref(),
            Some(path.as_path())
        );
    }

    #[test]
    fn locate_prefers_dream_bin() {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) { "dream.exe" } else { "dream" };
        let path = tmp.path().join(name);
        std::fs::write(&path, b"").unwrap();

        // SAFETY: every test in this crate that touches the environment holds `ENV_LOCK`.
        unsafe { std::env::set_var("DREAM_BIN", &path) };
        let located = locate().unwrap();
        // SAFETY: as above.
        unsafe { std::env::remove_var("DREAM_BIN") };
        assert_eq!(located, path);
    }
}
