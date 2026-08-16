fn main() {
    println!("cargo:rerun-if-changed=c/dream_rt.c");
    println!("cargo:rerun-if-changed=c/dream_host.c");
    println!("cargo:rerun-if-changed=c/dream_rt.h");
    cc::Build::new()
        .file("c/dream_rt.c")
        .file("c/dream_host.c")
        .include("c")
        .warnings(true)
        .compile("dream_rt_heap");
}
