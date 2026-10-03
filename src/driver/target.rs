//! Driver target selection; shared target data lives in `dream-abi` to keep MIR independent.

use dream_abi::target::{OsVersion, TargetSpec};
use dream_mir::backend::Target;

pub fn resolve(wasm: bool, min_os: Option<OsVersion>) -> Result<Target, String> {
    resolve_triple(wasm, None, min_os)
}

pub fn resolve_triple(
    wasm: bool,
    triple: Option<&str>,
    min_os: Option<OsVersion>,
) -> Result<Target, String> {
    let spec = if let Some(triple) = triple {
        let spec = TargetSpec::parse(triple)?;
        if spec.capabilities.linear_memory {
            return Err(
                "--target emits native objects; use --wasm for linear-memory output".into(),
            );
        }
        spec
    } else if wasm {
        TargetSpec::wasm32()
    } else {
        TargetSpec::host()
    };
    let spec = match min_os {
        Some(version) => spec.with_min_os(version)?,
        None => spec,
    };
    Ok(Target::Llvm(spec))
}
