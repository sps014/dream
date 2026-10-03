mod abi_codegen;

fn main() {
    for source in [
        "abi_registry.rs",
        "abi_codegen.rs",
        "src/runtime/c/include/dream_abi.h",
        "../../runtime/src/abi.js",
    ] {
        println!("cargo:rerun-if-changed={source}");
    }
    let header = abi_codegen::header();
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("dream_abi.h"), &header).unwrap();
    for (path, expected) in [
        ("src/runtime/c/include/dream_abi.h", header),
        ("../../runtime/src/abi.js", abi_codegen::javascript()),
    ] {
        let actual = std::fs::read_to_string(path).unwrap_or_default();
        assert_eq!(
            actual, expected,
            "{path} is stale; run scripts/generate-abi.sh"
        );
    }
}
