//! Native single-file launcher for a Dream module embedded at build time.

fn main() {
    let icon: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedded_icon.png"));
    dream::execution::host::set_packaged_app_icon(icon);

    let bytes: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedded.wasm"));
    let abi: &str = include_str!(concat!(env!("OUT_DIR"), "/embedded.abi.json"));
    let abi = if abi.trim().is_empty() || abi.trim() == "{}" {
        None
    } else {
        Some(abi)
    };
    if let Err(e) = dream::execution::wasm_runner::execute_wasm_bytes_with_abi(bytes, abi) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
