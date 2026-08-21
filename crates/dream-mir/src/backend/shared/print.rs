//! Pretty-prints a compiled WASM binary as `.wat` text (wasmprinter wrapper).

pub fn print_wasm(bytes: &[u8]) -> String {
    wasmprinter::print_bytes(bytes)
        .unwrap_or_else(|e| crate::internal_error!("wasmprinter failed on encoded module: {e}"))
}
