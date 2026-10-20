#[path = "src/capabilities.rs"]
pub mod capabilities;
#[path = "src/gpu_bindings.rs"]
pub mod gpu_bindings;
#[path = "src/guest_contract.rs"]
pub mod guest_contract;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(std::env::args().nth(1).ok_or("missing repository root")?);
    let check = std::env::args().any(|argument| argument == "--check");
    let mut artifacts = gpu_bindings::artifacts();
    artifacts.extend([
        (
            "crates/dream-abi/src/guest_types.rs",
            guest_contract::rust_types(),
        ),
        (
            "runtime/src/guest_contract.js",
            guest_contract::javascript(),
        ),
        (
            "crates/dream-mir/src/runtime/c/include/dream_guest_contract.h",
            guest_contract::c_header(),
        ),
        (
            "crates/dream-abi/src/capability_types.rs",
            capabilities::rust_types(),
        ),
        (
            "crates/dream-mir/src/runtime/c/include/dream_capabilities.h",
            capabilities::c_header(),
        ),
        ("runtime/src/capabilities.js", capabilities::javascript()),
    ]);
    for (path, expected) in artifacts {
        let path = root.join(path);
        if check {
            if std::fs::read_to_string(&path)?.replace("\r\n", "\n") != expected {
                return Err(format!(
                    "{} is stale; run python scripts/generate-capabilities.py",
                    path.display()
                )
                .into());
            }
        } else {
            std::fs::write(path, expected)?;
        }
    }
    Ok(())
}
