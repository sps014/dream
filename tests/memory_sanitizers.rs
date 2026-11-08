#![cfg(feature = "native")]

use std::process::Command;

/// These programs intentionally violate unsafe contracts. A normal trap or nonzero exit
/// cannot prove that generated Dream memory accesses received sanitizer instrumentation.
#[test]
#[ignore = "requires the pinned native ASan runtime"]
fn generated_accesses_trip_address_sanitizer() {
    let programs = [
        (
            "overflow",
            r#"
module system.core;
import system;
@noinline @unsafe fun read(borrow values: int[], index: int): int {
    return Buffer.get_unchecked<int>(values, index);
}
@unsafe fun main(): void {
    let values = [42];
    System.println(read(values, values.length));
}
"#,
            "use-after-poison",
            "load",
        ),
        (
            "overflow-store",
            r#"
module system.core;
import system;
@noinline @unsafe fun write(borrow values: int[], index: int): void {
    Buffer.set_unchecked<int>(values, index, 42);
}
@unsafe fun main(): void {
    let values = [0];
    write(values, values.length);
    System.println(values[0]);
}
"#,
            "use-after-poison",
            "store",
        ),
        (
            "managed-freed",
            r#"
module system.core;
import system;
@noinline @unsafe fun read(borrow values: int[], index: int): int {
    return Buffer.get_unchecked<int>(values, index);
}
@unsafe fun main(): void {
    let values = [42];
    let alias = values;
    Buffer.free<int>(values);
    System.println(read(alias, 0));
}
"#,
            "use-after-poison",
            "load",
        ),
        (
            "freed",
            r#"
import system;
@noinline @unsafe fun read(pointer: RawPtr<int>): int {
    return pointer.read();
}
@unsafe fun main(): void {
    let pointer = RawPtr<int>.alloc_zeroed((usize)1);
    let alias = RawPtr<int>.from_address(pointer.address());
    pointer.free();
    System.println(read(alias));
}
"#,
            "heap-use-after-free",
            "load",
        ),
    ];
    let temp = tempfile::tempdir().unwrap();
    for release in [false, true] {
        for (name, source, expected, access) in programs {
            let path = temp.path().join(format!("{name}-{release}.dream"));
            std::fs::write(&path, source).unwrap();
            let mut compiler = Command::new(env!("CARGO_BIN_EXE_dream"));
            compiler.env("DREAM_NATIVE_SANITIZE", "address,undefined");
            compiler.env("DREAM_VERIFY_MIR", "1");
            if release {
                compiler.arg("--release");
            }
            let built = compiler
                .args(["--emit-opt-ir", "-o"])
                .arg(path.with_extension("ll"))
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                built.status.success(),
                "{}",
                String::from_utf8_lossy(&built.stderr)
            );
            let ir = std::fs::read_to_string(path.with_extension("opt.ll")).unwrap();
            assert!(
                ir.contains("sanitize_address") && ir.contains(&format!("@__asan_report_{access}")),
                "{name}: missing instrumentation"
            );
            let output = Command::new(path.with_extension("bin"))
                .env("ASAN_OPTIONS", "detect_leaks=0:halt_on_error=1")
                .env("UBSAN_OPTIONS", "halt_on_error=1")
                .output()
                .unwrap();
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                !output.status.success()
                    && stderr.contains("ERROR: AddressSanitizer")
                    && stderr.contains(expected),
                "{name}, release={release}: {stderr}"
            );
        }
    }
}

#[test]
#[ignore = "requires the pinned native TSan runtime"]
fn generated_accesses_receive_thread_instrumentation() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("thread.dream");
    std::fs::write(
        &source,
        r#"
import system;
class Counter {
    public value: int;
    public constructor() { this.value = 42; }
}
@noinline fun read(borrow counter: Counter): int { return counter.value; }
fun main(): void { System.println(read(Counter())); }
"#,
    )
    .unwrap();
    for release in [false, true] {
        let mut compiler = Command::new(env!("CARGO_BIN_EXE_dream"));
        compiler
            .env("DREAM_NATIVE_SANITIZE", "thread")
            .env("DREAM_VERIFY_MIR", "1");
        if release {
            compiler.arg("--release");
        }
        let output = compiler
            .args(["--emit-opt-ir", "-o"])
            .arg(source.with_extension("ll"))
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ir = std::fs::read_to_string(source.with_extension("opt.ll")).unwrap();
        assert!(
            ir.contains("@__tsan_read4") && ir.contains("@__tsan_func_entry"),
            "release={release}: generated reads are not instrumented"
        );
        let output = Command::new(source.with_extension("bin"))
            .env("TSAN_OPTIONS", "halt_on_error=1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).starts_with("42\n"));
    }
}
