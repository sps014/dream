use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::PathBuf;

pub fn output_dir() -> PathBuf {
    let target = std::env::var("TARGET").expect("Cargo target");
    let mut variables = Vec::new();
    for base in [
        "CMAKE_GENERATOR",
        "CMAKE_GENERATOR_PLATFORM",
        "CMAKE_GENERATOR_TOOLSET",
        "CMAKE_TOOLCHAIN_FILE",
        "CMAKE_PREFIX_PATH",
        "CMAKE",
    ] {
        for name in [
            base.to_owned(),
            format!("{base}_{target}"),
            format!("{base}_{}", target.replace('-', "_")),
            format!("HOST_{base}"),
            format!("TARGET_{base}"),
        ] {
            println!("cargo:rerun-if-env-changed={name}");
            if let Some(value) = std::env::var_os(&name) {
                variables.push((name, value));
            }
        }
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo output directory"));
    // CMake caches generator/toolchain choices, including inside FetchContent subbuilds.
    out.join("cmake").join(configuration_key(variables))
}

pub fn configuration_key(mut variables: Vec<(String, OsString)>) -> String {
    variables.sort();
    let mut hash = Sha256::new();
    for (name, value) in variables {
        for bytes in [name.as_bytes(), value.as_encoded_bytes()] {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
    }
    format!("{:x}", hash.finalize())[..16].to_owned()
}
