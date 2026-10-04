use super::ToolchainConfig;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(super) fn prefix(
    explicit: Option<OsString>,
    home: Option<&Path>,
    user: Option<&Path>,
) -> PathBuf {
    if let Some(p) = explicit {
        return p.into();
    }
    if let Some(p) = home {
        if p.file_name().and_then(|s| s.to_str()) == Some("bin") {
            if let Some(parent) = p.parent() {
                return parent.to_path_buf();
            }
        }
        let cargo_target = matches!(
            p.file_name().and_then(|s| s.to_str()),
            Some("debug" | "release")
        ) && p
            .parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            == Some("target");
        if !cargo_target {
            return p.to_path_buf();
        }
    }
    user.unwrap_or(Path::new(".")).join(".dream")
}

pub(super) fn runtime_sources(
    explicit: Option<OsString>,
    home: Option<&Path>,
    user: Option<&Path>,
) -> PathBuf {
    if let Some(p) = explicit {
        return p.into();
    }
    let source = PathBuf::from(dream_mir::runtime::SOURCE_RUNTIME_C_DIR);
    if source.join("core/include/dream_core.h").is_file() {
        return source;
    }
    let mut roots = Vec::new();
    if let Some(home) = home {
        roots.push(home.join("lib/runtime/c"));
        if home.file_name().and_then(|s| s.to_str()) == Some("bin") {
            if let Some(parent) = home.parent() {
                roots.push(parent.join("lib/runtime/c"));
            }
        }
    }
    if let Some(user) = user {
        roots.push(user.join(".dream/lib/runtime/c"));
    }
    roots
        .into_iter()
        .find(|p| p.join("core/include/dream_core.h").is_file())
        .unwrap_or(source)
}

pub(super) fn host_library_dirs(config: &ToolchainConfig) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut push = |dir: PathBuf| {
        if dir.is_absolute() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    };
    if let Some(exe) = &config.exe {
        for exe in exe.canonicalize().ok().iter().chain(std::iter::once(exe)) {
            if let Some(p) = exe.parent() {
                push(p.to_path_buf());
                if p.file_name().and_then(|s| s.to_str()) == Some("deps") {
                    if let Some(parent) = p.parent() {
                        push(parent.to_path_buf());
                    }
                }
            }
        }
    }
    if let Some(home) = &config.home {
        push(home.clone());
        push(home.join("bin"));
    }
    if let Some(bin) = &config.bin {
        if let Some(parent) = bin.parent() {
            push(parent.to_path_buf());
        }
    }
    if let Some(user) = &config.user_home {
        push(user.join(".dream/bin"));
    }
    dirs
}

impl ToolchainConfig {
    pub fn find_on_path(&self, name: &str) -> Option<PathBuf> {
        let exe = if cfg!(windows) && !name.ends_with(".exe") && !name.contains(['/', '\\']) {
            format!("{name}.exe")
        } else {
            name.to_string()
        };
        self.path
            .iter()
            .map(|dir| dir.join(&exe))
            .find(|p| p.is_file())
    }

    pub fn program(&self, value: Option<&OsString>) -> Option<PathBuf> {
        let p = PathBuf::from(value?);
        if p.is_file() {
            Some(p)
        } else {
            self.find_on_path(p.to_str()?)
        }
    }

    pub fn bundle_dir(&self) -> Option<PathBuf> {
        let exe = self.exe.as_ref()?;
        let exe = exe.canonicalize().unwrap_or_else(|_| exe.clone());
        let dir = exe.parent()?;
        [dir.join("lib/dream"), dir.join("../lib/dream")]
            .iter()
            .find(|d| d.join("rt").is_dir())
            .cloned()
    }

    pub fn native_rt_cache_root(&self) -> PathBuf {
        self.prefix.join("cache").join("native-rt")
    }

    pub fn generator_cache_root(&self) -> PathBuf {
        self.prefix.join("cache").join("generators")
    }

    pub fn host_library_dirs(&self) -> Vec<PathBuf> {
        host_library_dirs(self)
    }

    pub fn loader_path_key() -> &'static str {
        if cfg!(target_os = "macos") {
            "DYLD_LIBRARY_PATH"
        } else if cfg!(windows) {
            "PATH"
        } else {
            "LD_LIBRARY_PATH"
        }
    }
}
