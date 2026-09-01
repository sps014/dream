//! Concurrency regression test for the panic-hook suppression in `driver::quiet_panic`.
//!
//! The old implementation swapped the process-global panic hook per `compile()` call and
//! serialized the swap with a mutex held across the entire codegen phase — so parallel
//! compiles funnelled single-file through their heaviest section. The thread-local design
//! must keep N concurrent full compiles correct (each ICE-catch window suppresses only its
//! own thread's panic output).

use dream::driver::compiler::{Compiler, Target};
use std::fs;
use std::path::PathBuf;
use std::thread;

fn temp_source(dir: &std::path::Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).expect("write source");
    path
}

const PROGRAM: &str = r#"
module main;

import system;
import system.collections;

fun main(): void {
    let xs: List<int> = [1, 2, 3];
    System.println(xs.join("+"));
}
"#;

#[test]
fn concurrent_native_compiles_all_succeed() {
    let dir = std::env::temp_dir().join("dream_concurrent_compile_test");
    fs::create_dir_all(&dir).expect("mkdir");

    const N: usize = 8;
    let mut handles = Vec::with_capacity(N);
    for i in 0..N {
        let dir = dir.clone();
        handles.push(thread::spawn(move || {
            let src = temp_source(
                &dir,
                &format!("prog_{i}.dream"),
                &PROGRAM.replace("xs.join(\"+\")", &format!("xs.join(\"+{i}\")")),
            );
            let out = dir.join(format!("out_{i}"));
            let compiler = Compiler::new(Target::NativeC);
            compiler
                .compile(
                    &src.to_string_lossy().to_string(),
                    &out.to_string_lossy().to_string(),
                )
                .map_err(|e| format!("{e}"))
        }));
    }

    for handle in handles {
        let result = handle.join().expect("worker panicked");
        assert!(
            result.is_ok(),
            "concurrent compile failed: {:?}",
            result.err()
        );
    }

    // Cleanup best-effort; failures here must not fail the test.
    let _ = fs::remove_dir_all(&dir);
}
