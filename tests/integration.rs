//! Every integration test, built as one binary so the `dream` library links once.

mod common;
mod dap;

mod build_cache;
mod compile_metrics;
mod compiler_dependencies;
mod compiler_properties;
mod concurrent_compile;
mod cross_emission;
mod cross_link;
mod debugger_dap;
mod debugger_dap_generator;
mod destructor_facts;
mod e2e_tests;
mod generator_cache;
mod host_capability_tests;
mod host_library_tests;
mod inline_value_types_test;
mod library_outputs;
mod llvm_backend_tests;
mod lock_emission;
mod mir_pipeline;
mod native_dead_strip;
mod native_interop;
mod native_pointer_emission;
mod noinline_attribute_tests;
mod publication_emission;
mod rc_elision_goldens;
mod runtime_callback;
mod runtime_cycles;
mod runtime_machine_size;
mod runtime_panic;
mod runtime_platform;
mod runtime_publish;
mod runtime_region;
mod runtime_sync;
mod runtime_wasi_platform;
mod runtime_weak;
mod runtime_worker;
mod sema_emission_tests;
mod structural_symbols;
mod take_borrow_tests;
mod target_spec;
mod wasm_interop;
mod wasm_opt_test;
mod weak_emission;

#[test]
fn every_test_file_is_a_module() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let listed = include_str!("integration.rs");
    for entry in std::fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.extension().is_some_and(|e| e == "rs") && stem != "integration" {
            assert!(
                listed.contains(&format!("\nmod {stem};\n")),
                "tests/{stem}.rs is not built: add `mod {stem};` to tests/integration.rs"
            );
        }
    }
}
