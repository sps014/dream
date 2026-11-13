include!("../dream-host-abi/link.rs");

fn main() {
    link_capability("drawing");
    let profile = cargo_profile_dir();
    // The uplifted cdylib lands in the profile directory. Cargo 1.99 also links tests from `deps`.
    for directory in [profile.clone(), profile.join("deps")] {
        std::fs::create_dir_all(&directory).expect("profile directory");
        std::fs::copy(
            "../dream-drawing/fonts/OFL.txt",
            directory.join("NotoSans-OFL.txt"),
        )
        .expect("stage bundled font license beside the drawing host");
    }
    println!("cargo:rerun-if-changed=../dream-drawing/fonts/OFL.txt");
}
