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

/// Includes embedded codegen sources so protocol edits invalidate executables and result caches.
pub fn compiler_identity(config: &ToolchainConfig) -> String {
    let sources = dream_stdlib::STD_PACKAGES
        .iter()
        .find(|p| p.name == "system.codegen")
        .map(|p| p.files)
        .unwrap_or_default();
    identity_with_sources(config, sources)
}

fn identity_with_sources(config: &ToolchainConfig, sources: &[(&str, &str)]) -> String {
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
    for (path, source) in sources {
        for part in [*path, *source] {
            hash.update(&(part.len() as u64).to_le_bytes());
            hash.update(part.as_bytes());
        }
    }
    hash.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_protocol_edits_invalidate_executable_and_result_keys() {
        let config = ToolchainConfig::default();
        let before =
            identity_with_sources(&config, &[("gen_model.dream", "public class Model {}")]);
        let after = identity_with_sources(
            &config,
            &[(
                "gen_model.dream",
                "public class Model { public field: int; }",
            )],
        );
        assert_ne!(before, after);
        assert_ne!(
            super::super::incremental::result_key(&before, "same snapshot"),
            super::super::incremental::result_key(&after, "same snapshot")
        );
    }
}
