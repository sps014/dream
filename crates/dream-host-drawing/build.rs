include!("../dream-host-abi/link.rs");

fn main() {
    link_capability("drawing");
    let directory = std::env::var("DEP_DREAM_HOST_CORE_LIBRARY_DIR").expect("core host directory");
    std::fs::copy(
        "../dream-drawing/fonts/OFL.txt",
        std::path::Path::new(&directory).join("NotoSans-OFL.txt"),
    )
    .expect("stage bundled font license beside the drawing host");
    println!("cargo:rerun-if-changed=../dream-drawing/fonts/OFL.txt");
}
