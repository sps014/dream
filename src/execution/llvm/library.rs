use super::*;
use std::process::Command;

pub(super) fn exports(abi: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(abi)?)?;
    Ok(value["exports"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect())
}

pub(super) fn archive(
    tools: &LlvmTools,
    out: &Path,
    object: &Path,
    native: &[PathBuf],
    vendor: Option<&Path>,
) -> Result<(), String> {
    // Replacing the archive prevents deleted native sources leaving stale members behind.
    if out.exists() {
        std::fs::remove_file(out).map_err(|e| e.to_string())?;
    }
    let mut ar = Command::new(tools.optional_tool("llvm-ar")?);
    ar.arg("qcsL")
        .arg(out)
        .arg(object)
        .args(native)
        .args(vendor);
    let result = run_captured(&mut ar, "llvm-ar (staticlib)");
    if result.is_err() {
        let _ = std::fs::remove_file(out);
    }
    result
}

pub(super) fn link_metadata(ll: &Path, flags: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(
        ll.with_extension("link.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"link_args": flags}))?,
    )?;
    Ok(())
}

pub(super) fn shared_flags(
    cmd: &mut Command,
    ll: &Path,
    out: &Path,
    exports: &[String],
    spec: &dream_abi::target::TargetSpec,
) -> Result<PathBuf, String> {
    let mut public = EMBED_EXPORTS
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    public.extend(exports.iter().cloned());
    let list = ll.with_extension(if spec.is_windows() { "def" } else { "exports" });
    let content = if spec.is_apple() {
        cmd.arg("-dynamiclib")
            .arg(format!(
                "-Wl,-install_name,@rpath/{}",
                out.file_name().unwrap_or_default().to_string_lossy()
            ))
            .arg(format!("-Wl,-exported_symbols_list,{}", list.display()));
        public.iter().map(|s| format!("_{s}\n")).collect::<String>()
    } else if spec.is_windows() {
        cmd.arg("-shared");
        let import = out.with_extension("lib");
        if spec.is_msvc() {
            cmd.arg(format!("-Wl,/def:{}", list.display()))
                .arg(format!("-Wl,/implib:{}", import.display()));
        } else {
            cmd.arg(&list)
                .arg(format!("-Wl,--out-implib,{}", import.display()));
        }
        format!("EXPORTS\n{}", public.join("\n"))
    } else {
        cmd.arg("-shared")
            .arg(format!(
                "-Wl,-soname,{}",
                out.file_name().unwrap_or_default().to_string_lossy()
            ))
            .arg(format!("-Wl,--version-script={}", list.display()));
        format!("{{ global: {}; local: *; }};\n", public.join("; "))
    };
    std::fs::write(&list, content).map_err(|e| e.to_string())?;
    Ok(list)
}
