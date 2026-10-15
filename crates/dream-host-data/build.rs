include!("../dream-host-abi/link.rs");
fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("wasm32") {
        link_capability("data");
    }
}
