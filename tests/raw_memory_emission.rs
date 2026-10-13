use crate::common::{compile_test_pipeline_for, emit_ll_for, ir_func_body};
use dream_mir::backend::Target;

#[test]
fn raw_memory_access_has_no_arc_bounds_checks_or_alias_promises() {
    // The single-source harness has no file identity for internal module access.
    let source = format!(
        "{}\n{}\n{}",
        include_str!("../crates/dream-stdlib/src/system/core/raw_memory.dream")
            .replace("module system.core;", "")
            .replace("internal ", "public "),
        include_str!("../crates/dream-stdlib/src/system/core/raw_ptr.dream")
            .replace("module system.core;", ""),
        r#"
        @unsafe @export fun read_at(p: RawPtr<int>, index: isize): int { return p[index]; }
        @unsafe @export fun write_at(p: RawPtr<int>, value: int): void { p.write(value); }
        "#
    );
    for target in [Target::native(), Target::wasm32()] {
        let (ir, names) = compile_test_pipeline_for(&source, target.clone(), |hir, i| {
            let mut mir = dream_mir::lower::lower_program(hir, i);
            mir.profile = dream_abi::profile::CompileProfile::Release;
            dream_mir::passes::optimize_module(&mut mir, i);
            dream_mir::passes::run_function_pipelines(
                &mut mir,
                i,
                &dream_mir::passes::PassManager::default_pipeline(),
                &dream_mir::passes::PassManager::default_pipeline(),
                &mut dream_mir::passes::MirDump::disabled(),
            );
            let names = ["read_at", "write_at"].map(|name| {
                mir.functions
                    .iter()
                    .find(|f| f.name == name)
                    .expect("raw access function")
                    .symbol
                    .clone()
            });
            (emit_ll_for(&mir, i, target), names)
        });
        for name in &names {
            let body = ir_func_body(&ir, name);
            assert!(!body.is_empty(), "{name}");
            for unwanted in [
                "dream_retain",
                "dream_release",
                "dream_array_at",
                "getelementptr inbounds",
                "!alias.scope",
                "!tbaa",
            ] {
                assert!(!body.contains(unwanted), "{name}: {unwanted}\n{body}");
            }
        }
        assert!(
            ir_func_body(&ir, &names[0]).contains("load i32"),
            "{}",
            ir_func_body(&ir, &names[0])
        );
        assert!(ir_func_body(&ir, &names[1]).contains("store i32"));
    }
}
