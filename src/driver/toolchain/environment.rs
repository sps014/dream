use super::*;

impl Default for ToolchainConfig {
    fn default() -> Self {
        Self::from_lookup(
            |key| std::env::var_os(key),
            std::env::current_exe().ok(),
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        )
    }
}

impl ToolchainConfig {
    pub(super) fn from_lookup(
        lookup: impl Fn(&str) -> Option<OsString>,
        exe: Option<PathBuf>,
        cwd: PathBuf,
    ) -> Self {
        let value = |key| lookup(key).filter(|v| !v.is_empty());
        let home = value("DREAM_HOME").map(PathBuf::from);
        let user_home = if cfg!(windows) {
            value("USERPROFILE").or_else(|| value("HOME"))
        } else {
            value("HOME").or_else(|| value("USERPROFILE"))
        }
        .map(PathBuf::from);
        let prefix = paths::prefix(value("DREAM_PREFIX"), home.as_deref(), user_home.as_deref());
        let mut toolchains = Vec::new();
        if let Some(root) = value("DREAM_TOOLCHAINS") {
            toolchains.push(PathBuf::from(root));
        }
        let default_tools = prefix.join("toolchains");
        if !toolchains.contains(&default_tools) {
            toolchains.push(default_tools);
        }
        let runtime_c = paths::runtime_sources(
            value("DREAM_RUNTIME_C"),
            home.as_deref(),
            user_home.as_deref(),
        );
        Self {
            runtime_c,
            home,
            bin: value("DREAM_BIN").map(PathBuf::from),
            llvm: value("DREAM_LLVM").map(PathBuf::from),
            toolchains,
            prefix,
            user_home,
            exe,
            cwd,
            path: value("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            cc: value("DREAM_CC").or_else(|| value("CC")),
            cxx: value("DREAM_CXX").or_else(|| value("CXX")),
            zig: value("DREAM_ZIG").map(PathBuf::from),
            no_auto_install: value("DREAM_NO_AUTO_INSTALL").is_some(),
            native_sanitize: value("DREAM_NATIVE_SANITIZE"),
            asan_options: lookup("ASAN_OPTIONS"),
            loader_path: lookup(Self::loader_path_key()),
            sdkroot: value("SDKROOT"),
            #[cfg(feature = "native")]
            sdkroot_args: OnceLock::new(),
            #[cfg(windows)]
            windir: value("WINDIR").map(PathBuf::from),
            #[cfg(feature = "native")]
            resolved_llvm: OnceLock::new(),
            #[cfg(feature = "native")]
            resolved_cc: OnceLock::new(),
        }
    }
}
