//! Pretty-prints a compiled WASM binary as `.wat` text (wasmprinter wrapper).

pub fn print_wasm(bytes: &[u8]) -> Result<String, String> {
    wasmprinter::print_bytes(bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn malformed_modules_return_errors_without_panicking() {
        assert!(super::print_wasm(b"invalid wasm").is_err());
    }
}
