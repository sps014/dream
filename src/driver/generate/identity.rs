//! Compiler identity for generator cache keys: a rebuilt `dream`, an edited C runtime, or a
//! different LLVM toolchain must miss every cached generator executable and result.

use crate::driver::toolchain::ToolchainConfig;
use std::sync::OnceLock;

/// The running compiler binary (which embeds the stdlib sources), by path, size and mtime.
fn exe_fingerprint() -> &'static str {
    static EXE: OnceLock<String> = OnceLock::new();
    EXE.get_or_init(|| match std::env::current_exe() {
        Ok(exe) => crate::driver::rt_stamp::fingerprint(vec![exe]),
        Err(_) => String::new(),
    })
}

pub fn compiler_identity(config: &ToolchainConfig) -> String {
    let mut hash = blake3::Hasher::new();
    for part in [
        env!("CARGO_PKG_VERSION"),
        exe_fingerprint(),
        &crate::driver::rt_stamp::fingerprint(crate::driver::rt_stamp::files_under(
            &config.runtime_c,
        )),
        &config.location_independent_fingerprint(),
    ] {
        hash.update(&(part.len() as u64).to_le_bytes());
        hash.update(part.as_bytes());
    }
    hash.finalize().to_hex().to_string()
}
