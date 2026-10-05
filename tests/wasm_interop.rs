use dream::driver::compiler::Compiler;
use dream::driver::wasm_opt::OptLevel;
use dream_mir::backend::Target;
use std::path::{Path, PathBuf};

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn compile(entry: &Path, output: &Path, optimize: Option<OptLevel>) -> Result<(), String> {
    Compiler::new(Target::wasm32())
        .with_optimize(optimize)
        .with_opt_ir(true)
        .compile(
            &entry.to_string_lossy().to_string(),
            &output.to_string_lossy(),
        )
        .map(drop)
        .map_err(|e| {
            e.diagnostic_text()
                .map(str::to_string)
                .unwrap_or_else(|| e.to_string())
        })
}

fn run(wat: &Path) -> String {
    let script = wat.with_extension("mjs");
    std::fs::write(&script, format!(
        "import {{pathToFileURL}} from 'node:url';\nconst {{run}} = await import(pathToFileURL({}));\nawait run(pathToFileURL({}), {{stdout: s => process.stdout.write(s)}});\n",
        serde_json::to_string(&repo("runtime/dream.js").to_string_lossy()).unwrap(),
        serde_json::to_string(&wat.with_extension("wasm").to_string_lossy()).unwrap(),
    )).unwrap();
    let output = std::process::Command::new("node")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .replace("\r\n", "\n")
}

fn write(root: &Path, file: &str, contents: &str) {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

#[test]
fn c_package_callbacks_strings_and_owned_pointer_drop() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("main.wat");
    compile(&repo("sample/native_c/src/main.dream"), &out, None).unwrap();
    assert_eq!(run(&out).trim(), "demo total = 30\ndemo total = 70\ncallbacks = 5\ndemo#4\n(none)\nfingerprint = 726441\n[c] ticker_free(demo)");
}

#[test]
fn wasi_libc_callbacks_and_pointer_marshalling() {
    for name in ["c_qsort_callback", "c_array_memchr"] {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("main.wat");
        compile(&repo(&format!("tests/cases/{name}.dream")), &out, None).unwrap();
        assert_eq!(
            run(&out).trim(),
            std::fs::read_to_string(repo(&format!("tests/cases/{name}.expected")))
                .unwrap()
                .trim()
        );
    }
}

#[test]
fn cpp_package_ownership_callbacks_layout_and_exceptions_at_all_levels() {
    for level in [None, Some(OptLevel::O3), Some(OptLevel::Size)] {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("main.wat");
        compile(&repo("sample/native_cpp/src/main.dream"), &out, level).unwrap();
        assert_eq!(run(&out).trim(), "changed: a,ab,b\nab = 22\nzz = (none)\nb as int = 333\ncount = 3\ncount(a) = 2\nscaled = 30\nsum = 10\nnorm = 5\norigin = 1.5,-2\ncompact ok true\nsnapshot failed\nsnapshot ab=22\nstore out of scope\nborrowed entry ab\n[c++] ~Store(main.db)\nkv 1.0\nhello, dream\n[c++] ~Store(default)\ncompact failed: nothing to compact\n[c++] ~Store(empty)");
    }
}

#[test]
fn c_bitcode_inlines_and_preserves_aggregate_and_allocator_abi() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "dream.toml", "[package]\nname = \"portable\"\n[native.portable.windows]\ndefines = [\"HOST_ONLY=1\"]\n[native.portable.wasm]\ndefines = [\"GUEST_ONLY=1\"]\n");
    write(
        root,
        "main.dream",
        r#"
import system;
public struct Pair { public x: int; public y: int; }
@c extern fun transform(p: Pair): Pair;
@c extern fun arithmetic(x: int): int;
@c extern fun invoke(f: fun(int): int, x: int): int;
@c extern fun memory_test(): int;
fun twice(x: int): int { return x * 2; }
fun main(): int {
    let input = Pair();
    input.x = 3;
    input.y = 7;
    let p = transform(input);
    System.println(p.x.to_string() + "," + p.y.to_string());
    System.println(arithmetic(9));
    System.println(invoke(twice, 4));
    System.println(memory_test());
    return 0;
}
"#,
    );
    write(
        root,
        "native/portable.c",
        r#"
#include <stdint.h>
#include <stdlib.h>
#if !defined(GUEST_ONLY) || defined(HOST_ONLY)
#error wrong target settings
#endif
typedef struct { int x; int y; } Pair;
Pair transform(Pair p) { return (Pair){ p.y, p.x }; }
int arithmetic(int x) { return x * 3 + 1; }
int invoke(int (*fn)(int), int x) { return fn(x); }
int memory_test(void) {
    char *p = calloc(5, 1);
    if (!p || (uintptr_t)p % 16) return -1;
    p[0] = 42;
    p = realloc(p, 200);
    if (!p || p[0] != 42) return -2;
    free(p);
    void *q = NULL;
    if (posix_memalign(&q, 64, 100) || (uintptr_t)q % 64) return -3;
    free(q);
    return 42;
}
"#,
    );
    let out = root.join("main.wat");
    compile(&root.join("main.dream"), &out, Some(OptLevel::O3)).unwrap();
    assert_eq!(run(&out), "7,3\n28\n8\n42\n");
    let ir = std::fs::read_to_string(out.with_extension("opt.ll")).unwrap();
    assert!(
        !ir.contains("call i32 @arithmetic("),
        "C body should inline into Dream"
    );
    let before = std::fs::read(out.with_extension("wasm")).unwrap();
    compile(&root.join("main.dream"), &out, Some(OptLevel::O3)).unwrap();
    assert_eq!(before, std::fs::read(out.with_extension("wasm")).unwrap());
}

#[test]
fn cpp_global_constructors_and_destructors_run_once() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "dream.toml", "[package]\nname = \"lifetime\"\n");
    write(root, "main.dream", "@cpp(\"lifetime.hpp\", \"value\") extern fun value(): int;\nfun main(): int { System.println(value()); return 0; }\n");
    write(root, "native/include/lifetime.hpp", "int value();\n");
    write(root, "native/lifetime.cpp", "#include <cstdio>\nstruct Global { Global() { std::puts(\"constructed\"); } ~Global() { std::puts(\"destroyed\"); } };\nGlobal global;\nint value() { return 7; }\n");
    let out = root.join("main.wat");
    compile(&root.join("main.dream"), &out, None).unwrap();
    assert_eq!(run(&out), "constructed\n7\ndestroyed\n");
}

#[test]
fn unsupported_package_function_is_a_link_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "dream.toml", "[package]\nname = \"missing\"\n");
    write(
        root,
        "main.dream",
        "@c extern fun unavailable(): int;\nfun main(): int { return unavailable(); }\n",
    );
    write(
        root,
        "native/missing.c",
        "extern int platform_only(void);\nint unavailable(void) { return platform_only(); }\n",
    );
    let out = root.join("main.wat");
    let err = compile(&root.join("main.dream"), &out, None).unwrap_err();
    assert!(
        err.contains("unavailable function 'env.platform_only'"),
        "{}",
        err
    );
    assert!(!out.with_extension("wasm").exists());
}

#[test]
fn shared_memory_package_interop_initializes_tls() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "dream.toml", "[package]\nname = \"parallel\"\n");
    write(
        root,
        "main.dream",
        r#"
import system;
import system.task;
@c extern fun value(): int;
@c extern fun set_error(n: int): void;
@c extern fun get_error(): int;
async fun main(): void {
    System.println(value());
    let first = Task.spawn(() => value());
    let second = Task.spawn(() => value());
    System.println(first.await);
    System.println(second.await);
    System.println(value());
    let pool = TaskPool(1);
    System.println(pool.dispatch(() => value().to_string()).await);
    System.println(pool.dispatch(() => value().to_string()).await);
    pool.shutdown();
    set_error(1234);
    System.println(Task.spawn(() => get_error()).await);
    System.println(get_error());
}

"#,
    );
    write(
        root,
        "native/value.c",
        r#"
#include <stdint.h>
#include <errno.h>
#include <stdlib.h>
static _Thread_local int counter = 41;
static _Thread_local _Alignas(256) volatile unsigned char scratch[70000];
void set_error(int n) { errno = n; }
int get_error(void) { return errno; }
int value(void) {
    if ((uintptr_t)scratch % 256 || scratch[69999] != counter - 41) return -1;
    char *end;
    errno = 0;
    (void)strtol("999999999999999999999999999", &end, 10);
    if (errno != ERANGE) return -2;
    scratch[69999]++;
    return ++counter;
}
"#,
    );
    for level in [None, Some(OptLevel::O3), Some(OptLevel::Size)] {
        let out = root.join("main.wat");
        compile(&root.join("main.dream"), &out, level).unwrap();
        assert_eq!(run(&out), "42\n42\n42\n43\n42\n43\n0\n1234\n");
    }
}

#[test]
fn shared_memory_cpp_initialization_runs_once_after_tls_setup() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "dream.toml", "[package]\nname = \"parallel_cpp\"\n");
    write(root, "native/include/value.hpp", "int value();\n");
    write(
        root,
        "native/value.cpp",
        r#"
#include <cstdio>
static thread_local int counter = 41;
struct Global {
    Global() { counter = 100; std::puts("constructed"); }
    ~Global() { std::puts("destroyed"); }
};
static Global global;
int value() {
    try { throw counter; }
    catch (int previous) { return counter = previous + 1; }
}
"#,
    );
    write(
        root,
        "main.dream",
        r#"
import system;
import system.task;
@cpp("value.hpp", "value") extern fun value(): int;
async fun main(): void {
    System.println(value());
    System.println(Task.spawn(() => value()).await);
    System.println(value());
}
"#,
    );
    let out = root.join("main.wat");
    compile(&root.join("main.dream"), &out, Some(OptLevel::O3)).unwrap();
    assert_eq!(run(&out), "constructed\n101\n42\n102\ndestroyed\n");
}

#[test]
fn wrong_pointer_width_is_diagnosed_before_llvm_can_turn_it_into_a_trap() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "dream.toml", "[package]\nname = \"width\"\n");
    write(
        root,
        "main.dream",
        "@c extern fun width(n: long): int;\nfun main(): int { return width(1); }\n",
    );
    write(
        root,
        "native/width.c",
        "#include <stddef.h>\nint width(size_t n) { return (int)n; }\n",
    );
    let err = compile(&root.join("main.dream"), &root.join("main.wat"), None).unwrap_err();
    assert!(err.contains("C ABI mismatch for 'width'"), "{}", err);
    assert!(err.contains("usize/isize"), "{}", err);
}
