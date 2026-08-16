fn main() {
    cc::Build::new()
        .file("c/dream_rt.c")
        .file("c/dream_host.c")
        .include("c")
        .warnings(true)
        .compile("dream_rt");
}
