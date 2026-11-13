include!("../dream-host-abi/profile.rs");

fn main() {
    // Cargo supplies this profile path for both host and cross-target builds.
    let directory = cargo_profile_dir().join("deps");
    println!("cargo:library_dir={}", directory.display());
    // Cargo propagates cdylib-specific link arguments to dependents. Keep this
    // install identity local so capability libraries cannot inherit core's name.
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => println!("cargo:rustc-link-arg=-Wl,-soname,libdream_host_core.so"),
        Ok("android") => {
            println!("cargo:rustc-link-arg=-Wl,-soname,libdream_host_core.so");
            println!("cargo:rustc-link-arg=-Wl,-z,max-page-size=16384");
        }
        Ok("macos" | "ios") => {
            println!("cargo:rustc-link-arg=-Wl,-install_name,@rpath/libdream_host_core.dylib")
        }
        _ => {}
    }
}
