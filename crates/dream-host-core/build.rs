fn main() {
    // Cargo supplies this profile path for both host and cross-target builds.
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let directory = output
        .ancestors()
        .nth(3)
        .expect("Cargo profile directory")
        .join("deps");
    println!("cargo:library_dir={}", directory.display());
    // Cargo propagates cdylib-specific link arguments to dependents. Keep this
    // install identity local so capability libraries cannot inherit core's name.
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => println!("cargo:rustc-link-arg=-Wl,-soname,libdream_host_core.so"),
        Ok("macos") => {
            println!("cargo:rustc-link-arg=-Wl,-install_name,@rpath/libdream_host_core.dylib")
        }
        _ => {}
    }
}
