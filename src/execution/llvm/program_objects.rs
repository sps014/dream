use super::run_llc;
use crate::driver::{cache_budget, rt_stamp, wasm_opt::OptLevel};
use crate::execution::llvm::{runtime_objects::compile_objects, tools::LlvmTools};
use dream_abi::target::TargetSpec;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;

pub(super) fn compile(
    tools: &LlvmTools,
    spec: &TargetSpec,
    units: &[String],
) -> Result<Vec<PathBuf>, String> {
    let llc = rt_stamp::persistent_tool_identity(
        &tools.tool("llc"),
        &tools.config.tool_identity_cache_root(),
    )
    .ok_or("unreadable LLVM code generator")?;
    let configuration = format!("dream-debug-unit-v1:{spec:?}:{llc}");
    let cache = tools.config.native_rt_cache_root().join("program-objects");
    compile_objects(units, |_, unit| {
        let mut key = blake3::Hasher::new();
        key.update(configuration.as_bytes());
        key.update(unit.as_bytes());
        let directory = cache.join(key.finalize().to_hex().as_str());
        fs::create_dir_all(&directory).map_err(|err| err.to_string())?;
        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join(".lock"))
            .map_err(|err| err.to_string())?;
        lock.lock().map_err(|err| err.to_string())?;
        let object = directory.join("unit.o");
        if !object.is_file() {
            let input = directory.join("unit.ll");
            fs::write(&input, unit).map_err(|err| err.to_string())?;
            let temp =
                tempfile::NamedTempFile::new_in(&directory).map_err(|err| err.to_string())?;
            let result = run_llc(tools, &input, OptLevel::O0, false, "obj", temp.path(), spec);
            let _ = fs::remove_file(input);
            result?;
            temp.persist(&object).map_err(|err| err.to_string())?;
        }
        cache_budget::touch(&directory);
        Ok(object)
    })
}
