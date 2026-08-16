//! Portable ARC heap used by the LLVM backend (`dream-rt` C library).
//!
//! Guest references are `i32` offsets into a linear heap so MIR layouts stay identical to WASM.

mod guest;

#[cfg(not(target_arch = "wasm32"))]
pub mod host;

use std::os::raw::c_char;
use std::path::PathBuf;

extern "C" {
    pub fn dream_rt_init();
    pub fn dream_heap_base() -> *mut u8;
    pub fn dream_malloc(size: i32, tag: i32) -> i32;
    pub fn dream_retain(ptr: i32);
    pub fn dream_release(ptr: i32);
    pub fn dream_intern_utf8(bytes: *const c_char, len: i32) -> i32;
    pub fn dream_str_byte_size(ptr: i32) -> i32;
    pub fn dream_string_eq(a: i32, b: i32) -> i32;
    pub fn dream_concat_strings(a: i32, b: i32) -> i32;
    pub fn dream_print_int(v: i32);
}

/// Absolute path to the C sources so the LLVM driver can compile them with clang.
pub fn c_sources() -> [&'static str; 3] {
    [
        concat!(env!("CARGO_MANIFEST_DIR"), "/c/dream_rt.c"),
        concat!(env!("CARGO_MANIFEST_DIR"), "/c/dream_host.c"),
        concat!(env!("CARGO_MANIFEST_DIR"), "/c/entry.c"),
    ]
}

pub fn c_include_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/c")
}

/// `libdream_rt.a` for clang to link into native guest binaries (heap C + Rust hosts + libstd).
pub fn native_archive() -> PathBuf {
    let mut p = PathBuf::from(env!("OUT_DIR"));
    p.pop();
    p.pop();
    p.pop();
    p.join("libdream_rt.a")
}

/// System libraries rustc expects when linking a staticlib on this host.
pub fn native_sys_libs() -> &'static [&'static str] {
    #[cfg(target_os = "macos")]
    {
        &[
            "-lSystem",
            "-lc++",
            "-lresolv",
            "-framework",
            "Security",
            "-framework",
            "CoreFoundation",
            "-framework",
            "SystemConfiguration",
            "-framework",
            "AppKit",
            "-framework",
            "Metal",
            "-framework",
            "QuartzCore",
            "-framework",
            "IOKit",
            "-framework",
            "CoreVideo",
            "-framework",
            "CoreGraphics",
            "-framework",
            "Foundation",
            "-framework",
            "Carbon",
            "-framework",
            "WebKit",
            "-lffi",
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        &["-lpthread", "-ldl", "-lm", "-lstdc++", "-lffi"]
    }
    #[cfg(not(unix))]
    {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dream_mir::abi;

    #[test]
    fn header_matches_mir_abi() {
        assert_eq!(12, abi::HEAP_HEADER_SIZE);
        assert_eq!(4, abi::HEADER_TAG_OFFSET);
        assert_eq!(8, abi::HEADER_REFCOUNT_OFFSET);
        assert_eq!(1024, abi::STRING_BASE);
        assert_eq!(5, abi::TAG_STRING);
    }

    #[test]
    fn malloc_retain_release_and_strings() {
        unsafe {
            dream_rt_init();
            let p = dream_malloc(4, abi::TAG_INT);
            assert!(p > 0);
            dream_retain(p);
            dream_release(p);
            dream_release(p);
            let hello = b"hi";
            let s = dream_intern_utf8(hello.as_ptr() as *const c_char, hello.len() as i32);
            assert_eq!(dream_str_byte_size(s), 2);
            let s2 = dream_intern_utf8(hello.as_ptr() as *const c_char, hello.len() as i32);
            assert_eq!(dream_string_eq(s, s2), 1);
            let cat = dream_concat_strings(s, s2);
            assert_eq!(dream_str_byte_size(cat), 4);
        }
    }
}
