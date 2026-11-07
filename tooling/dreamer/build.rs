//! Embeds SDL's Android Java layer from the same pinned SDL sources the window host compiles, so
//! an APK's Java classes always match the native SDL inside `libdream_host_window.so`.

use std::fmt::Write;
use std::path::Path;

fn main() {
    let java = Path::new(sdl3_src::SOURCE_DIR).join("android-project/app/src/main/java");
    let package = java.join("org/libsdl/app");
    let mut names: Vec<_> = std::fs::read_dir(&package)
        .expect("reading SDL's Android Java sources")
        .map(|entry| entry.expect("listing SDL Java sources").file_name())
        .filter_map(|name| name.to_str().map(str::to_owned))
        .filter(|name| name.ends_with(".java"))
        .collect();
    names.sort();
    let mut out = String::from("pub const SDL_JAVA: &[(&str, &str)] = &[\n");
    for name in names {
        let path = package.join(&name);
        writeln!(
            out,
            "    (\"org/libsdl/app/{name}\", include_str!({:?})),",
            path.display().to_string()
        )
        .unwrap();
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("sdl_java.rs");
    std::fs::write(dest, out).expect("writing the embedded SDL Java table");
    println!("cargo:rerun-if-changed=build.rs");
}
