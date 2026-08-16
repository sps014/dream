//! LLVM target triples Dream knows how to ask clang for.

use std::env;

/// A parsed LLVM target triple (codegen, not `CompileTargets` native/node/web).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Triple {
    pub arch: String,
    pub vendor: String,
    pub os: String,
    pub env: Option<String>,
}

impl Triple {
    pub fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() < 3 {
            return Err(format!(
                "expected LLVM triple like wasm32-unknown-unknown or aarch64-apple-darwin, got '{}'",
                s
            ));
        }
        Ok(Triple {
            arch: parts[0].to_string(),
            vendor: parts[1].to_string(),
            os: parts[2].to_string(),
            env: if parts.len() > 3 {
                Some(parts[3..].join("-"))
            } else {
                None
            },
        })
    }

    pub fn is_wasm(&self) -> bool {
        self.arch.starts_with("wasm")
    }

    pub fn as_str(&self) -> String {
        match &self.env {
            Some(e) => format!("{}-{}-{}-{}", self.arch, self.vendor, self.os, e),
            None => format!("{}-{}-{}", self.arch, self.vendor, self.os),
        }
    }
}

/// Host triple inferred from `std::env::consts` (not a sysroot).
pub fn host_triple() -> Triple {
    let arch = env::consts::ARCH;
    match env::consts::OS {
        "macos" => Triple::parse(&format!("{}-apple-darwin", arch)).unwrap(),
        "linux" => Triple::parse(&format!("{}-unknown-linux-gnu", arch)).unwrap(),
        "windows" => Triple::parse(&format!("{}-pc-windows-msvc", arch)).unwrap(),
        _ => Triple::parse("x86_64-unknown-linux-gnu").unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_triples() {
        assert!(Triple::parse("wasm32-unknown-unknown").unwrap().is_wasm());
        assert_eq!(
            Triple::parse("aarch64-apple-darwin").unwrap().as_str(),
            "aarch64-apple-darwin"
        );
        assert_eq!(
            Triple::parse("x86_64-pc-windows-msvc").unwrap().as_str(),
            "x86_64-pc-windows-msvc"
        );
    }
}
