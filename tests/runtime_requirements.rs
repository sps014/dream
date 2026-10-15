#![cfg(feature = "native")]

use dream::driver::compiler::Compiler;
use dream_mir::backend::Target;

fn compile(source: &str) -> (tempfile::TempDir, String, serde_json::Value) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("main.dream");
    let output = dir.path().join("main.ll");
    std::fs::write(&input, source).unwrap();
    Compiler::new(Target::native())
        .with_release(true)
        .compile(&input.display().to_string(), output.to_str().unwrap())
        .unwrap();
    let ir = std::fs::read_to_string(&output).unwrap();
    let abi =
        serde_json::from_slice(&std::fs::read(output.with_extension("abi.json")).unwrap()).unwrap();
    (dir, ir, abi)
}

#[test]
fn unused_imports_do_not_require_argument_capture_or_callback_startup() {
    let (_dir, ir, abi) = compile("import system; import system.task; fun main(): void {}");
    assert!(!ir.contains("call void @dream_process_capture_args"));
    assert!(!ir.contains("call void @dream_thread_attach"));
    assert!(!ir.contains("call void @dream_callback_owner_finish"));
    assert!(!ir.contains("call void @dream_defer_drain_all"));
    let exports = abi["exports"].as_array().unwrap();
    for name in dream_abi::c_abi::EMBED_EXPORTS {
        assert!(!exports.iter().any(|e| e.as_str() == Some(name)));
    }
    assert!(ir.contains("@dream_llvm_leak_report"));
}

#[test]
fn reachable_arguments_keep_capture_without_callback_attachment() {
    let (_dir, ir, _) =
        compile("import system; fun main(): void { System.println(System.args.length); }");
    assert!(ir.contains("call void @dream_process_capture_args"));
    assert!(!ir.contains("call void @dream_thread_attach"));
}

#[test]
fn reachable_c_imports_keep_embedding_and_startup_services() {
    let (_dir, ir, abi) = compile(
        "import system.interop; @c(\"c\", \"abs\") extern fun absolute(x: int): int; fun main(): void { let n = absolute(-2); }",
    );
    assert!(ir.contains("call void @dream_thread_attach"));
    assert!(ir.contains("call void @dream_process_capture_args"));
    let exports = abi["exports"].as_array().unwrap();
    for name in dream_abi::c_abi::EMBED_EXPORTS {
        assert!(exports.iter().any(|e| e.as_str() == Some(name)));
    }
}

#[test]
fn executable_path_keeps_process_capture() {
    let (_dir, ir, _) =
        compile("import system; fun main(): void { System.println(System.exe_path); }");
    assert!(ir.contains("call void @dream_process_capture_args"));
    assert!(!ir.contains("call void @dream_thread_attach"));
}

#[test]
fn deferred_destruction_keeps_final_drain_before_reporting() {
    let (_dir, ir, _) = compile(
        "import system; class Note { public constructor() {} } fun main(): void { defer { let n = Note(); } }",
    );
    let main = ir.split("define i32 @main(").nth(1).unwrap();
    let drain = main.find("call void @dream_defer_drain_all").unwrap();
    let report = main.find("call void @dream_llvm_leak_report").unwrap();
    assert!(drain < report);
}

#[test]
fn wasm_c_imports_do_not_add_native_embedding_exports_to_the_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("main.dream");
    let output = dir.path().join("main.ll");
    std::fs::write(&input,
        "import system.interop; @c(\"c\", \"abs\") extern fun absolute(x: int): int; fun main(): int { return absolute(-2); }",
    ).unwrap();
    Compiler::new(Target::wasm32())
        .compile(&input.display().to_string(), output.to_str().unwrap())
        .unwrap();
    let abi: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.with_extension("abi.json")).unwrap()).unwrap();
    let exports = abi["exports"].as_array().unwrap();
    for name in dream_abi::c_abi::EMBED_EXPORTS {
        assert!(!exports.iter().any(|e| e.as_str() == Some(name)));
    }
}

#[test]
fn local_native_callback_keeps_owner_without_c_imports_or_argument_capture() {
    let (_dir, ir, _) = compile(include_str!("cases/native_callback_local.dream"));
    assert!(ir.contains("call void @dream_thread_attach"));
    assert!(ir.contains("call void @dream_callback_owner_finish"));
    assert!(!ir.contains("call void @dream_process_capture_args"));
}

#[test]
fn opaque_runtime_intrinsics_keep_owner_and_cleanup_services() {
    let (_dir, ir, _) = compile(
        "static class Runtime { @intrinsic(Intrinsic.StringClone) public static extern fun clone(borrow s: string): string; } fun main(): void { let copy = Runtime.clone(\"x\"); }",
    );
    assert!(ir.contains("call void @dream_thread_attach"));
    assert!(ir.contains("call void @dream_process_capture_args"));
    assert!(ir.contains("call void @dream_defer_drain_all"));
}

#[test]
fn empty_entry_selects_zero_heap_reporting_but_numeric_formatting_keeps_real_counters() {
    let (_dir, empty, _) = compile("fun main(): void {}");
    assert!(empty.contains("call void @dream_llvm_leak_report(i32 0, i32 0)"));
    for value in ["true", "123L", "123u", "123UL", "(byte)7"] {
        let (_dir, ir, _) = compile(&format!(
            "import system; fun main(): void {{ System.println({value}); }}"
        ));
        assert!(
            ir.contains("call void @dream_llvm_leak_report(i32 0, i32 1)"),
            "{value}"
        );
    }
}
