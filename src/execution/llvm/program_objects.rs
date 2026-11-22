use super::run_llc;
use crate::driver::{cache_budget, rt_stamp, wasm_opt::OptLevel};
use crate::execution::llvm::tools::LlvmTools;
use dream_abi::target::TargetSpec;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) fn compile(
    tools: &LlvmTools,
    spec: &TargetSpec,
    units: &[String],
) -> Result<Vec<PathBuf>, String> {
    let _phase = tracing::info_span!(
        "compile_phase",
        phase = "program_objects",
        units = units.len()
    )
    .entered();
    let llc = rt_stamp::persistent_tool_identity(
        &tools.tool("llc"),
        &tools.config.tool_identity_cache_root(),
    )
    .ok_or("unreadable LLVM code generator")?;
    let configuration = format!("dream-debug-unit-v1:{spec:?}:{llc}");
    let cache = tools.config.native_rt_cache_root().join("program-objects");
    largest_first(units, |unit| {
        let mut key = blake3::Hasher::new();
        key.update(configuration.as_bytes());
        key.update(unit.as_bytes());
        let directory = cache.join(key.finalize().to_hex().as_str());
        compile_unit(tools, spec, unit, &directory)
    })
}

fn compile_unit(
    tools: &LlvmTools,
    spec: &TargetSpec,
    unit: &str,
    directory: &Path,
) -> Result<PathBuf, String> {
    fs::create_dir_all(directory).map_err(|err| err.to_string())?;
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
        let temp = tempfile::NamedTempFile::new_in(directory).map_err(|err| err.to_string())?;
        let result = run_llc(tools, &input, OptLevel::O0, false, "obj", temp.path(), spec);
        let _ = fs::remove_file(input);
        result?;
        temp.persist(&object).map_err(|err| err.to_string())?;
    }
    cache_budget::touch(directory);
    Ok(object)
}

/// Runs `compile` over every unit on all cores, starting the largest first so one big unit does
/// not begin last and finish alone. Results keep `units` order.
fn largest_first(
    units: &[String],
    compile: impl Fn(&str) -> Result<PathBuf, String> + Sync,
) -> Result<Vec<PathBuf>, String> {
    let mut order: Vec<usize> = (0..units.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(units[i].len()));
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(units.len().max(1));
    let finished: Vec<Vec<(usize, Result<PathBuf, String>)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    while let Some(&i) = order.get(next.fetch_add(1, Ordering::Relaxed)) {
                        done.push((i, compile(&units[i])));
                    }
                    done
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or_default())
            .collect()
    });
    let mut objects: Vec<Option<Result<PathBuf, String>>> = vec![None; units.len()];
    for (i, result) in finished.into_iter().flatten() {
        objects[i] = Some(result);
    }
    objects
        .into_iter()
        .map(|object| object.unwrap_or_else(|| Err("codegen unit worker panicked".into())))
        .collect()
}
