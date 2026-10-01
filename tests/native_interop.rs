//! Native C/C++ packages: `native/` sources, `@c` wrappers, and `@cpp` shims. Tests that compile C
//! or C++ need a C/C++ compiler (Zig, or `DREAM_CXX`) and run with `--ignored`; the rest only
//! drive the front end.

use dream::driver::compiler::{Compiler, Target};
use dream::driver::wasm_opt::OptLevel;
use dream::execution::native::compile_and_capture_ex;
use std::fs;
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn out_dir(tag: &str) -> PathBuf {
    let dir = repo("target/native-interop").join(tag);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes `files` (relative path, contents) under a fresh temp directory.
fn project(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "dream-native-interop-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    for (rel, text) in files {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    root
}

fn compile(target: Target, entry: &Path, out: &Path) -> Result<(), String> {
    Compiler::new(target)
        .compile(
            &entry.to_str().unwrap().to_string(),
            &out.to_str().unwrap().to_string(),
        )
        .map_err(|e| {
            e.diagnostic_text()
                .map(str::to_string)
                .unwrap_or_else(|| e.to_string())
        })
}

fn build(entry: &Path, tag: &str) -> Result<PathBuf, String> {
    let ll = out_dir(tag).join("main.ll");
    compile(Target::Native, entry, &ll)?;
    Ok(ll)
}

fn run(entry: &Path, tag: &str) -> Result<String, String> {
    let ll = build(entry, tag)?;
    // A freshly linked binary's first launch can be slow while the OS vets it.
    compile_and_capture_ex(ll.to_str().unwrap(), OptLevel::O0, &[], &[], None, 60)
        .map_err(|e| e.to_string())
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected {:?} in:\n{}",
        needle,
        haystack
    );
}

#[test]
#[ignore = "builds C/C++ sources; cargo test --workspace -- --ignored"]
fn c_package_keeps_a_stored_native_callback_alive() {
    let out = run(&repo("sample/native_c/src/main.dream"), "c_sample").unwrap();
    assert_eq!(
        out.trim(),
        "demo total = 30\n\
         demo total = 70\n\
         callbacks = 5\n\
         demo#4\n\
         (none)\n\
         fingerprint = 726441\n\
         [c] ticker_free(demo)"
    );
}

#[test]
#[ignore = "builds C/C++ sources; cargo test --workspace -- --ignored"]
fn cpp_package_runs_through_the_generated_shim() {
    let out = run(&repo("sample/native_cpp/src/main.dream"), "cpp_sample").unwrap();
    assert_eq!(
        out.trim(),
        "changed: a,ab,b\n\
         ab = 22\n\
         zz = (none)\n\
         b as int = 333\n\
         count = 3\n\
         count(a) = 2\n\
         scaled = 30\n\
         sum = 10\n\
         norm = 5\n\
         origin = 1.5,-2\n\
         compact ok true\n\
         snapshot failed\n\
         snapshot ab=22\n\
         store out of scope\n\
         borrowed entry ab\n\
         [c++] ~Store(main.db)\n\
         kv 1.0\n\
         hello, dream\n\
         [c++] ~Store(default)\n\
         compact failed: nothing to compact\n\
         [c++] ~Store(empty)"
    );
}

#[test]
#[ignore = "builds C/C++ sources; cargo test --workspace -- --ignored"]
fn cpp_declaration_mismatch_is_reported_at_the_dream_line() {
    let root = project(
        "mismatch",
        &[
            ("dream.toml", "[package]\nname = \"boxes\"\n"),
            (
                "native/include/box.hpp",
                "namespace m {\nstruct Box {\n    explicit Box(int v) : v_(v) {}\n    int get() const { return v_; }\n    int v_;\n};\n}\n",
            ),
            ("native/box.cpp", "#include \"box.hpp\"\n"),
            (
                "src/main.dream",
                "import system;\n\n@cpp(\"box.hpp\", \"m::Box\")\nclass Box {\n    extern constructor(v: int);\n    extern fun value(): int;\n}\n\nfun main(): void {\n    System.println(Box(3).value());\n}\n",
            ),
        ],
    );
    let err = run(&root.join("src/main.dream"), "mismatch").unwrap_err();
    assert_contains(&err, "main.dream:6");
    assert_contains(&err, "value");
    let _ = fs::remove_dir_all(&root);
}

#[test]
#[ignore = "builds C/C++ sources; cargo test --workspace -- --ignored"]
fn cpp_struct_layout_mismatch_fails_the_static_assert() {
    let root = project(
        "layout",
        &[
            ("dream.toml", "[package]\nname = \"geo\"\n"),
            (
                "native/include/p.hpp",
                "struct P { double x; double y; };\ninline double len2(P p) { return p.x * p.x + p.y * p.y; }\n",
            ),
            ("native/p.cpp", "#include \"p.hpp\"\n"),
            (
                "src/main.dream",
                "import system;\n\n@cpp(\"p.hpp\", \"P\")\npublic struct P {\n    public x: float;\n    public y: float;\n}\n\n@cpp(\"p.hpp\", \"len2\")\nextern fun len2(p: P): double;\n\nfun main(): void {\n    System.println(len2(P()));\n}\n",
            ),
        ],
    );
    let err = run(&root.join("src/main.dream"), "layout").unwrap_err();
    assert_contains(&err, "@cpp struct 'P': sizeof(P) differs from the Dream declaration");
    let _ = fs::remove_dir_all(&root);
}

#[test]
#[ignore = "builds C/C++ sources; cargo test --workspace -- --ignored"]
fn callback_from_a_foreign_thread_traps() {
    let root = project(
        "thread",
        &[
            ("dream.toml", "[package]\nname = \"threads\"\n"),
            (
                "native/thread.c",
                "#include <pthread.h>\n\
                 typedef int (*cb_fn)(void*, int);\n\
                 struct job { cb_fn fn; void* user; };\n\
                 static void* worker(void* p) { struct job* j = p; j->fn(j->user, 7); return 0; }\n\
                 void call_from_thread(cb_fn fn, void* user) {\n\
                     pthread_t t; struct job j = { fn, user };\n\
                     pthread_create(&t, 0, worker, &j); pthread_join(t, 0);\n\
                 }\n",
            ),
            (
                "src/main.dream",
                "import system;\n\n@c extern fun call_from_thread(cb: NativeCallback<fun(int): int>): void;\n\nfun main(): void {\n    let cb = NativeCallback<fun(int): int>((x: int) => x + 1);\n    call_from_thread(cb);\n    System.println(\"unreachable\");\n}\n",
            ),
        ],
    );
    let err = run(&root.join("src/main.dream"), "thread").unwrap_err();
    assert_contains(&err, "C callbacks must run on their Dream owner thread");
    let _ = fs::remove_dir_all(&root);
}

/// A package binding its own C++ `<name>::Store` as the Dream class `class`.
fn store_package(name: &str, class: &str) -> [(String, String); 4] {
    [
        (
            format!("dream_packages/{name}/dream.toml"),
            format!("[package]\nname = \"{name}\"\n"),
        ),
        (
            format!("dream_packages/{name}/native/include/{name}.hpp"),
            format!(
                "namespace {name} {{\nstruct Store {{\n    const char* name() const {{ return \"{name}\"; }}\n}};\n}}\n"
            ),
        ),
        (
            format!("dream_packages/{name}/native/{name}.cpp"),
            format!("#include \"{name}.hpp\"\n"),
        ),
        (
            format!("dream_packages/{name}/src/{name}.dream"),
            format!(
                "module {name};\n\n@cpp(\"{name}.hpp\", \"{name}::Store\")\npublic class {class} {{\n    extern constructor();\n    extern fun name(): string;\n}}\n"
            ),
        ),
    ]
}

#[test]
#[ignore = "builds C/C++ sources; cargo test --workspace -- --ignored"]
fn two_packages_can_each_bind_a_cpp_store() {
    let mut files: Vec<(String, String)> = vec![
        ("dream.toml".into(), "[package]\nname = \"app\"\n".into()),
        (
            "src/main.dream".into(),
            "import system;\nimport alpha;\nimport beta;\n\nfun main(): void {\n    System.println(AlphaStore().name() + \" \" + BetaStore().name());\n}\n"
                .into(),
        ),
    ];
    files.extend(store_package("alpha", "AlphaStore"));
    files.extend(store_package("beta", "BetaStore"));
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let root = project("two_stores", &refs);
    let out = run(&root.join("src/main.dream"), "two_stores").unwrap();
    assert_eq!(out.trim(), "alpha beta");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn links_conflict_names_both_manifests() {
    let root = project(
        "links",
        &[
            ("dream.toml", "[package]\nname = \"app\"\n"),
            ("src/main.dream", "fun main(): void {}\n"),
            (
                "dream_packages/a/dream.toml",
                "[package]\nname = \"a\"\nlinks = \"z\"\n",
            ),
            (
                "dream_packages/b/dream.toml",
                "[package]\nname = \"b\"\nlinks = \"z\"\n",
            ),
        ],
    );
    let err = build(&root.join("src/main.dream"), "links").unwrap_err();
    assert_contains(&err, "native library 'z' is provided by two packages");
    assert_contains(&err, "dream_packages/a/dream.toml");
    assert_contains(&err, "dream_packages/b/dream.toml");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cpp_shim_and_ir_are_deterministic() {
    let entry = repo("sample/native_cpp/src/main.dream");
    let read = |tag: &str| {
        let ll = build(&entry, tag).unwrap();
        let shim = ll.parent().unwrap().join("native-c/kvstore/shim.cpp");
        (
            fs::read_to_string(&ll).unwrap(),
            fs::read_to_string(shim).unwrap(),
        )
    };
    let (ll_a, shim_a) = read("det_a");
    let (ll_b, shim_b) = read("det_b");
    assert!(shim_a == shim_b, "shim.cpp differs between two compiles");
    assert!(ll_a == ll_b, ".ll differs between two compiles");
    assert_contains(&shim_a, "extern \"C\"");
    assert_contains(&shim_a, "#line ");
}

#[test]
fn wasm32_rejects_a_live_cpp_member_by_its_dream_name() {
    let entry = repo("sample/native_cpp/src/main.dream");
    let out = out_dir("wasm").join("main.wat");
    let err = compile(Target::Wasm32, &entry, &out).unwrap_err();
    assert_contains(
        &err,
        "'Store.put' is a native C/C++ import and cannot be called from a wasm32 build",
    );
    assert_contains(&err, "kvstore.dream");
    assert!(!err.contains("dream__"), "{}", err);
}
