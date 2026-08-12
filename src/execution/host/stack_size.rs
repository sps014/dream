//! Resolve Dream's WASM call-stack size for wasmtime.
//!
//! Precedence:
//! 1. Runtime env `DREAM_STACK_SIZE` (e.g. `32M`, `32MiB`, `33554432`)
//! 2. Compile-time default from `[package.metadata.dream] stack-size` in the root `Cargo.toml`
//!    (baked in by `build.rs` as `DREAM_DEFAULT_STACK_SIZE`)
//! 3. Hard fallback: 16 MiB (historical default)

/// Default when neither env nor Cargo metadata is set.
pub const FALLBACK_STACK_BYTES: usize = 16 * 1024 * 1024;

/// Bytes for `Config::max_wasm_stack`.
pub fn dream_stack_size() -> usize {
    if let Ok(raw) = std::env::var("DREAM_STACK_SIZE") {
        if let Some(n) = parse_size_bytes(&raw) {
            return n.max(64 * 1024); // reject absurdly small values
        }
        eprintln!(
            "warning: ignoring invalid DREAM_STACK_SIZE={:?}; expected bytes or K/M/G suffix",
            raw
        );
    }
    option_env!("DREAM_DEFAULT_STACK_SIZE")
        .and_then(parse_size_bytes)
        .unwrap_or(FALLBACK_STACK_BYTES)
        .max(64 * 1024)
}

/// Async fiber stack: a few MiB above the sync WASM stack (matches the old 16→20 MiB pairing).
pub fn dream_async_stack_size() -> usize {
    dream_stack_size().saturating_add(4 * 1024 * 1024)
}

/// Parse `16777216`, `16M`, `16MB`, `16MiB`, `16K`, etc. into a byte count.
pub fn parse_size_bytes(raw: &str) -> Option<usize> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let upper = s.to_ascii_uppercase();
    let (num_part, mult) = if let Some(rest) = upper.strip_suffix("MIB") {
        (rest, 1024usize * 1024)
    } else if let Some(rest) = upper.strip_suffix("MB") {
        (rest, 1000 * 1000)
    } else if let Some(rest) = upper.strip_suffix('M') {
        (rest, 1024 * 1024)
    } else if let Some(rest) = upper.strip_suffix("KIB") {
        (rest, 1024)
    } else if let Some(rest) = upper.strip_suffix("KB") {
        (rest, 1000)
    } else if let Some(rest) = upper.strip_suffix('K') {
        (rest, 1024)
    } else if let Some(rest) = upper.strip_suffix("GIB") {
        (rest, 1024 * 1024 * 1024)
    } else if let Some(rest) = upper.strip_suffix("GB") {
        (rest, 1000 * 1000 * 1000)
    } else if let Some(rest) = upper.strip_suffix('G') {
        (rest, 1024 * 1024 * 1024)
    } else if let Some(rest) = upper.strip_suffix('B') {
        (rest, 1)
    } else {
        (upper.as_str(), 1)
    };
    let n: usize = num_part.trim().parse().ok()?;
    n.checked_mul(mult)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_bytes() {
        assert_eq!(parse_size_bytes("16777216"), Some(16 * 1024 * 1024));
    }

    #[test]
    fn parses_m_suffix() {
        assert_eq!(parse_size_bytes("16M"), Some(16 * 1024 * 1024));
        assert_eq!(parse_size_bytes("16MiB"), Some(16 * 1024 * 1024));
        assert_eq!(parse_size_bytes("16MB"), Some(16_000_000));
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(parse_size_bytes(""), None);
        assert_eq!(parse_size_bytes("  "), None);
    }
}
