//! Driver target selection; shared target data lives in `dream-abi` to keep MIR independent.

use dream_abi::target::{OsVersion, TargetSpec};
use dream_mir::backend::Target;

pub fn resolve(wasm: bool, min_os: Option<OsVersion>) -> Result<Target, String> {
    let spec = if wasm {
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
