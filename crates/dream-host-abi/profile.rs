/// Profile directory (`target/debug` or `target/<triple>/release`).
///
/// Cargo 1.99: `…/<profile>/build/<pkg>-<hash>/out`.
/// Cargo 1.100: `…/<profile>/build/<pkg>/<hash>/out`.
fn cargo_profile_dir() -> std::path::PathBuf {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    out.ancestors()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("build"))
        .and_then(|build| build.parent())
        .expect("Cargo profile directory")
        .to_path_buf()
}
