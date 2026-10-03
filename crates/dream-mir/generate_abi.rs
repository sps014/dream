mod abi_codegen;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(std::env::args().nth(1).ok_or("missing repository root")?);
    let check = std::env::args().any(|arg| arg == "--check");
    for (path, expected) in [
        (
            "crates/dream-mir/src/runtime/c/include/dream_abi.h",
            abi_codegen::header(),
        ),
        ("runtime/src/abi.js", abi_codegen::javascript()),
    ] {
        let path = root.join(path);
        if check {
            if std::fs::read_to_string(&path)? != expected {
                return Err(
                    format!("{} is stale; run scripts/generate-abi.sh", path.display()).into(),
                );
            }
        } else {
            std::fs::write(path, expected)?;
        }
    }
    Ok(())
}
