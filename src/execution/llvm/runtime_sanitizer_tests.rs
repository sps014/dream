use super::*;

#[test]
fn sanitized_bitcode_defers_global_registration_until_after_internalization() {
    let config = std::sync::Arc::new(crate::driver::toolchain::ToolchainConfig {
        native_sanitize: Some("address,undefined".into()),
        ..Default::default()
    });
    let Ok(tools) = super::super::resolve_llvm(&config) else {
        eprintln!("skipping: pinned LLVM not installed");
        return;
    };
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("globals.c");
    std::fs::write(
        &source,
        "const int global_map[42] = {1, 2, 3};\nint read_map(int index) { return global_map[index]; }\n",
    )
    .unwrap();
    for opt in [OptLevel::O0, OptLevel::O3] {
        let bitcode = temp.path().join("globals.bc");
        let mut clang = tools.command("clang");
        clang
            .args(["--target=x86_64-unknown-linux-gnu", "-c"])
            .args(bitcode_flags(
                &config,
                &TargetSpec::host(),
                opt,
                &clang_level_flags(opt),
            ))
            .args(["-fsanitize=address,undefined", "-fno-sanitize-recover=all"])
            .arg(&source)
            .arg("-o")
            .arg(&bitcode);
        run_captured(&mut clang, "clang (sanitizer regression)").unwrap();
        let before = disassemble(&tools, &bitcode).unwrap();
        assert!(before.contains("sanitize_address"));
        assert!(before.contains("__ubsan_handle_out_of_bounds_abort"));
        assert!(!before.contains("@asan.module_ctor"));

        let optimized = temp.path().join("globals.opt.bc");
        let mut command = tools.command("opt");
        command
            .arg(format!(
                "-passes={},asan",
                super::super::build::pipeline(opt)
            ))
            .arg("-internalize-public-api-list=read_map")
            .arg(&bitcode)
            .arg("-o")
            .arg(&optimized);
        run_captured(&mut command, "opt (sanitizer regression)").unwrap();
        let after = disassemble(&tools, &optimized).unwrap();
        assert!(after.contains("@___asan_globals_registered = common hidden global"));
        assert!(after.contains("@__asan_register_elf_globals"));
        assert!(after.contains("@__asan_report_load4"));
    }
}
