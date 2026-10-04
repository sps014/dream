//! Native C/C++ packages: `native/` sources, `@c` wrappers, and `@cpp` shims. Tests that compile C
//! or C++ need a C/C++ compiler (Zig, `DREAM_CXX`, or the system `clang++`/`c++`).

use dream::driver::compiler::Compiler;
use dream::driver::wasm_opt::OptLevel;
use dream::execution::native::compile_and_capture_ex;
use dream_mir::backend::Target;
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
    let root =
        std::env::temp_dir().join(format!("dream-native-interop-{tag}-{}", std::process::id()));
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
            out.to_str().unwrap(),
        )
        .map_err(|e| {
            e.diagnostic_text()
                .map(str::to_string)
                .unwrap_or_else(|| e.to_string())
        })
        .map(drop)
}

fn build(entry: &Path, tag: &str) -> Result<PathBuf, String> {
    let ll = out_dir(tag).join("main.ll");
    compile(Target::native(), entry, &ll)?;
    Ok(ll)
}

fn run(entry: &Path, tag: &str) -> Result<String, String> {
    run_at(entry, tag, OptLevel::O0)
}

fn run_at(entry: &Path, tag: &str, level: OptLevel) -> Result<String, String> {
    let ll = build(entry, tag)?;
    // A freshly linked binary's first launch can be slow while the OS vets it.
    compile_and_capture_ex(
        &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        ll.to_str().unwrap(),
        level,
        &[],
        &[],
        None,
        60,
    )
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
    assert_contains(
        &err,
        "@cpp struct 'P': sizeof(P) differs from the Dream declaration",
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn callback_from_an_unattached_foreign_thread_traps() {
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
    assert_contains(
        &err,
        "a NativeCallback closure must run on the Dream thread that created it",
    );
    let _ = fs::remove_dir_all(&root);
}

/// One `@c` package around `native/abi.c`, run at `level`.
fn run_c(tag: &str, c: &str, dream: &str, level: OptLevel) -> Result<String, String> {
    let root = project(
        tag,
        &[
            ("dream.toml", "[package]\nname = \"abi\"\n"),
            ("native/abi.c", c),
            ("src/main.dream", dream),
        ],
    );
    let out = run_at(&root.join("src/main.dream"), tag, level);
    let _ = fs::remove_dir_all(&root);
    out
}

#[test]
fn c_structs_pass_and_return_by_value() {
    let c = r#"#include <stdint.h>
typedef struct { float x, y; } Vec2;
typedef struct { int64_t a, b, c; } Triple;
typedef struct { double w; int32_t id; int64_t big; float f; } Wide;
Vec2 vec2_scale(Vec2 v, float k) { Vec2 r = { v.x * k, v.y * k }; return r; }
Triple triple_rotate(Triple t) { Triple r = { t.b, t.c, t.a }; return r; }
Wide wide_bump(Wide w, int32_t by) { w.w += 0.5; w.id += by; w.big *= 2; w.f -= 1.0f; return w; }
"#;
    let dream = r#"import system;

public struct Vec2 { public x: float; public y: float; }
public struct Triple { public a: long; public b: long; public c: long; }
public struct Wide { public w: double; public id: int; public big: long; public f: float; }

@c extern fun vec2_scale(v: Vec2, k: float): Vec2;
@c extern fun triple_rotate(t: Triple): Triple;
@c extern fun wide_bump(w: Wide, by: int): Wide;

fun main(): void {
    let v = Vec2();
    v.x = 1.5f;
    v.y = -2.0f;
    let s = vec2_scale(v, 2.0f);
    System.println("vec2 " + s.x.to_string() + " " + s.y.to_string());
    let t = Triple();
    t.a = 1L;
    t.b = 5000000000L;
    t.c = -3L;
    let r = triple_rotate(t);
    System.println("triple " + r.a.to_string() + " " + r.b.to_string() + " " + r.c.to_string());
    let w = Wide();
    w.w = 1.25;
    w.id = 40;
    w.big = 21L;
    w.f = 3.5f;
    let b = wide_bump(w, 2);
    System.println("wide " + b.w.to_string() + " " + b.id.to_string() + " " + b.big.to_string() + " " + b.f.to_string());
}
"#;
    for (tag, level) in [("structs_o0", OptLevel::O0), ("structs_o2", OptLevel::O2)] {
        let out = run_c(tag, c, dream, level).unwrap();
        assert_eq!(
            out.trim(),
            "vec2 3 -4\ntriple 5000000000 -3 1\nwide 1.75 42 42 2.5"
        );
    }
}

#[test]
fn c_bool_return_ignores_garbage_upper_bits() {
    // Declared to Dream as returning `bool`: only the low byte carries the value. Darwin arm64 makes
    // the callee zero-extend a `bool` return, so garbage there would break the C ABI itself.
    let c = r#"#include <stdint.h>
#if defined(__APPLE__) && defined(__aarch64__)
#define GARBAGE 0u
#else
#define GARBAGE 0xABCDEF00u
#endif
uint32_t is_even_raw(int32_t x) { return GARBAGE | (uint32_t)(x % 2 == 0); }
"#;
    let dream = r#"import system;

@c("abi", "is_even_raw") extern fun is_even(x: int): bool;

fun main(): void {
    System.println(is_even(4).to_string() + " " + is_even(7).to_string());
    let n = 0;
    for (let i = 0; i < 10; i++) {
        if is_even(i) {
            n += 1;
        }
    }
    System.println(n.to_string());
}
"#;
    let out = run_c("narrow_bool", c, dream, OptLevel::O2).unwrap();
    assert_eq!(out.trim(), "true false\n5");
}

#[test]
fn attached_foreign_thread_calls_a_dream_function() {
    let c = r#"#include <pthread.h>
#include <stdint.h>
#include <dream_embed.h>
typedef int32_t (*int_fn)(int32_t);
struct job { int_fn fn; int32_t out; };
static void* worker(void* p) {
    struct job* j = p;
    dream_thread_attach();
    j->out = j->fn(20);
    dream_thread_detach();
    return 0;
}
int32_t call_on_thread(int_fn fn) {
    pthread_t t; struct job j = { fn, 0 };
    pthread_create(&t, 0, worker, &j); pthread_join(t, 0);
    return j.out;
}
"#;
    let dream = r#"import system;

@c extern fun call_on_thread(f: fun(int): int): int;

fun label_len(x: int): int {
    let s = "value " + x.to_string();
    return s.length + x;
}

fun main(): void {
    System.println(call_on_thread(label_len).to_string());
}
"#;
    let out = run_c("attach", c, dream, OptLevel::O0).unwrap();
    assert_eq!(out.trim(), "28");
}

#[test]
fn owned_c_pointer_is_freed_by_its_finalizer() {
    let c = r#"#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
typedef struct { int32_t v; } Res;
void* res_new(int32_t v) { Res* r = malloc(sizeof *r); r->v = v; return r; }
int32_t res_value(void* r) { return ((Res*)r)->v; }
void res_free(void* r) { printf("[c] res_free(%d)\n", ((Res*)r)->v); fflush(stdout); free(r); }
"#;
    let dream = r#"import system;

@c @owned("res_free") extern fun res_new(v: int): OwnedCPtr;
@c extern fun res_value(r: CPtr): int;
@c extern fun res_free(r: CPtr): void;

fun scoped(): void {
    let r = res_new(7);
    System.println("value " + res_value(r.get()).to_string());
}

fun main(): void {
    scoped();
    System.println("after scope");
    let raw = res_new(8).take();
    System.println("taken " + res_value(raw).to_string());
    res_free(raw);
}
"#;
    let out = run_c("owned", c, dream, OptLevel::O0).unwrap();
    assert_eq!(
        out.trim(),
        "value 7\n[c] res_free(7)\nafter scope\ntaken 8\n[c] res_free(8)"
    );
}

#[test]
fn panic_hook_runs_then_the_process_aborts() {
    let c = r#"#include <stdio.h>
#include <dream_embed.h>
static void hook(const char* message, const char* location) {
    printf("[hook] %s (%s)\n", message, location ? location : "no location");
    fflush(stdout);
}
void install_hook(void) { dream_set_panic_hook(hook); }
"#;
    let dream = r#"import system;

@c extern fun install_hook(): void;

fun main(): void {
    install_hook();
    System.println("before");
    System.panic("boom é");
    System.println("unreachable");
}
"#;
    let err = run_c("panic_hook", c, dream, OptLevel::O0).unwrap_err();
    assert_contains(&err, "before");
    assert_contains(&err, "[hook] boom é (");
    assert_contains(&err, "src/main.dream:8)");
    assert!(!err.contains("unreachable"), "{}", err);
}

#[test]
fn cpp_callback_destroyed_on_a_foreign_thread_releases_on_its_owner() {
    let root = project(
        "callback-release",
        &[
            ("dream.toml", "[package]\nname = \"callback_release\"\n"),
            (
                "native/include/drop.hpp",
                "#include <functional>\n#include <thread>\ninline void drop_on_thread(std::function<int(int)> callback) { std::thread([callback = std::move(callback)]() mutable { callback = {}; }).join(); }\n",
            ),
            ("native/drop.cpp", "#include \"drop.hpp\"\n"),
            (
                "src/main.dream",
                "import system;\n@cpp(\"drop.hpp\", \"drop_on_thread\") extern fun drop_on_thread(callback: fun(int): int): void;\nclass Capture { public constructor() {} public fun value(): int { return 7; } del() { System.println(\"capture destroyed\"); } }\nfun submit(): void { let capture = Capture(); drop_on_thread((x: int) => capture.value() + x); System.println(\"foreign release queued\"); }\nfun main(): void { submit(); }\n",
            ),
        ],
    );
    let output = run(&root.join("src/main.dream"), "callback-release").unwrap();
    assert_eq!(output.trim(), "foreign release queued\ncapture destroyed");
    let entry = root.join("src/main.dream");
    let source = fs::read_to_string(&entry).unwrap().replace(
        "fun main(): void { submit(); }",
        "async fun main(): void { submit(); Time.delay(1).await; System.println(\"drained\"); }",
    );
    fs::write(&entry, source).unwrap();
    let output = run(&entry, "callback-release-async").unwrap();
    assert_eq!(
        output.trim(),
        "foreign release queued\ncapture destroyed\ndrained"
    );
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
    for package in ["a", "b"] {
        let manifest = root.join("dream_packages").join(package).join("dream.toml");
        let manifest = fs::canonicalize(manifest).unwrap();
        assert_contains(&err, &manifest.display().to_string());
    }
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
    let err = compile(Target::wasm32(), &entry, &out).unwrap_err();
    assert_contains(
        &err,
        "'Store.put' is a native C/C++ import and cannot be called from a wasm32 build",
    );
    assert_contains(&err, "kvstore.dream");
    assert!(!err.contains("dream__"), "{}", err);
}
