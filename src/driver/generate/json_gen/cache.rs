use super::harness::HARNESS_SOURCE;

pub(super) fn fnv1a(parts: &[&str]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in p.as_bytes() {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h ^= 0xff;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub(super) fn cached_harness_ll(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
) -> Result<String, String> {
    let fingerprint = fnv1a(&[
        HARNESS_SOURCE,
        include_str!("../../abi.rs"),
        include_str!("../../../../crates/dream-abi/src/host_capability.rs"),
        include_str!("../../../../crates/dream-abi/src/host_capability_fields.rs"),
        include_str!("../../../../crates/dream-stdlib/src/lib.rs"),
        include_str!("../../../../crates/dream-stdlib/src/packages.rs"),
        include_str!("../../../../crates/dream-stdlib/src/functions.rs"),
        include_str!("../../../../crates/dream-stdlib/src/source_paths.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/codegen.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/collections.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/core.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/crypto.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/desktop.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/encoding.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/gpu.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/io.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/json.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/logging.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/mod.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/net.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/primitives.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/process.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/simd.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/system.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/task.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/text.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/webapi.rs"),
        include_str!("../../../../crates/dream-stdlib/src/registry/webview.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/inline/eligibility.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/inline/graph.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/inline/pipeline.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/inline/remap.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/inline/splice.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/elision/branches.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/elision/chains.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/elision/mod.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/elision/pipeline.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/elision/postdom.rs"),
        include_str!("../../../../crates/dream-stdlib/src/system/json/json_generator.dream"),
        include_str!("../../../../crates/dream-stdlib/src/system/json/gen_result.dream"),
        include_str!("../../../../crates/dream-stdlib/src/system/json/gen_field.dream"),
        include_str!("../../../../crates/dream-stdlib/src/system/json/gen_collection.dream"),
        include_str!("../../../../crates/dream-stdlib/src/system/json/gen_variant.dream"),
        include_str!("../../../../crates/dream-stdlib/src/system/json/gen_type.dream"),
        include_str!("../../../../crates/dream-stdlib/src/system/codegen/codegen.dream"),
        include_str!("../../../../crates/dream-stdlib/src/system/core/string_builder.dream"),
        include_str!("../../../../crates/dream-stdlib/src/system/json/json_value.dream"),
        include_str!("../../../../crates/dream-stdlib/src/system/json/json.dream"),
        include_str!("../../../../crates/dream-stdlib/src/system/json/json_parser.dream"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/insertion/mod.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/insertion/prepare.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/insertion/blocks.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/insertion/exits.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/insertion/awaits.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/insertion/helpers.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/inline/mod.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/module_pipeline.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/sroa/mod.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/sroa/managed.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/frame_alloc.rs"),
        include_str!("../../../../crates/dream-hir/src/module.rs"),
        include_str!("../../../../crates/dream-sema/src/analyzer/hir_emit/mod.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/held.rs"),
        include_str!("../../../../crates/dream-mir/src/analysis/escape.rs"),
        include_str!("../../../../crates/dream-mir/src/analysis/object_life.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/uniqueness.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/tokens/aliases.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/tokens/analysis.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/tokens/calls.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/tokens/destroy.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/tokens/flow.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/tokens/locals.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/tokens/mod.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/tokens/statements.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/lifetime.rs"),
        include_str!("../../../../crates/dream-mir/src/passes/rc/cursor.rs"),
        include_str!("../../../../crates/dream-mir/src/backend/llvm/mod.rs"),
        include_str!("../../../../crates/dream-mir/src/backend/llvm/lcx.rs"),
        include_str!("../../../../crates/dream-mir/src/backend/llvm/body.rs"),
        include_str!("../../../../crates/dream-mir/src/backend/llvm/rvalue.rs"),
        include_str!("../../../../crates/dream-mir/src/backend/llvm/statements.rs"),
        include_str!("../../../../crates/dream-mir/src/backend/llvm/places.rs"),
        include_str!("../../../../crates/dream-mir/src/backend/llvm/calls.rs"),
        include_str!("../../../../crates/dream-mir/src/backend/llvm/terminator.rs"),
        include_str!("../../../../crates/dream-mir/src/backend/llvm/glue/release.rs"),
        include_str!("../../../../crates/dream-mir/src/backend/llvm/glue/entry.rs"),
        &format!(
            "{}:{}",
            dream_mir::abi::STRING_HEADER_SIZE,
            dream_mir::abi::STRING_UNITS_OFFSET
        ),
    ]);
    let dir = super::super::manifest::harness_cache_dir(config, "json-gen-harness", fingerprint);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("@json generator: create harness dir: {e}"))?;
    let lock_path = dir.join(".lock");
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| format!("@json generator: lock harness dir: {e}"))?;
    lock_file
        .lock()
        .map_err(|e| format!("@json generator: lock harness dir: {e}"))?;
    let src_path = dir.join("harness.dream");
    let ll_path = dir.join("harness.ll");
    if !ll_path.is_file() {
        std::fs::write(&src_path, HARNESS_SOURCE)
            .map_err(|e| format!("@json generator: write harness source: {e}"))?;
        let src = src_path.to_string_lossy().into_owned();
        let out = ll_path.to_string_lossy().into_owned();
        let compiler = crate::driver::compiler::Compiler::new_with_toolchain_config(
            dream_mir::backend::Target::native(),
            config.clone(),
        )
        .with_skip_generators(true)
        .with_release(true)
        .with_optimize(Some(crate::driver::wasm_opt::OptLevel::O0));
        compiler
            .compile(&src, &out)
            .map_err(|_| "@json generator: failed to compile Dream harness".to_string())?;
    }
    Ok(ll_path.to_string_lossy().into_owned())
}
