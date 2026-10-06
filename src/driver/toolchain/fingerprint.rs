use super::ToolchainConfig;

impl ToolchainConfig {
    pub fn fingerprint(&self) -> String {
        let mut hash = blake3::Hasher::new();
        for value in [
            format!(
                "{:?}",
                (
                    &self.runtime_c,
                    &self.home,
                    &self.bin,
                    &self.llvm,
                    &self.toolchains,
                    &self.prefix,
                    &self.exe,
                    &self.user_home
                )
            ),
            format!(
                "{:?}",
                (
                    &self.cwd,
                    &self.path,
                    &self.cc,
                    &self.cxx,
                    &self.zig,
                    &self.wasm_opt,
                    &self.targets,
                    &self.sysroot
                )
            ),
            format!(
                "{:?}",
                (
                    &self.sdkroot,
                    &self.developer_dir,
                    self.no_auto_install,
                    &self.native_sanitize
                )
            ),
        ] {
            hash.update(value.as_bytes());
        }
        hash.finalize().to_hex().to_string()
    }
}
