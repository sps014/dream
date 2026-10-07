use super::ToolchainConfig;

impl ToolchainConfig {
    /// Everything that selects tools and libraries, including the working directory.
    pub fn fingerprint(&self) -> String {
        let mut hash = blake3::Hasher::new();
        hash.update(self.location_independent_fingerprint().as_bytes());
        hash.update(format!("{:?}", self.cwd).as_bytes());
        hash.finalize().to_hex().to_string()
    }

    /// [`Self::fingerprint`] without the working directory, for caches shared by every project
    /// (generator executables and results), so a moved or second project still hits them.
    pub fn location_independent_fingerprint(&self) -> String {
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
        hash.update(format!("{:?}", self.compiler_environment).as_bytes());
        hash.finalize().to_hex().to_string()
    }
}
