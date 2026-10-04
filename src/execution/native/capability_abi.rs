use dream_abi::{host_capability::HostCapability, target::TargetSpec};
use object::Object;
use std::path::Path;

pub(crate) fn validate(
    directory: &Path,
    capabilities: &[HostCapability],
    spec: &TargetSpec,
) -> Result<(), String> {
    for capability in capabilities {
        let path = directory.join(capability.library_name(spec));
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file = object::File::parse(bytes.as_slice())
            .map_err(|e| format!("{}: invalid capability library: {e}", path.display()))?;
        let expected = match spec.triple.architecture.to_string().as_str() {
            "aarch64" | "arm64" => object::Architecture::Aarch64,
            "x86_64" => object::Architecture::X86_64,
            arch => return Err(format!("unsupported native capability architecture {arch}")),
        };
        let format = if spec.is_windows() {
            object::BinaryFormat::Pe
        } else if spec.is_apple() {
            object::BinaryFormat::MachO
        } else {
            object::BinaryFormat::Elf
        };
        if file.architecture() != expected || file.format() != format {
            return Err(format!(
                "{}: capability library does not match target {}",
                path.display(),
                spec.triple
            ));
        }
        let marker = format!(
            "{}dream_host_{}_abi_v2",
            if spec.is_apple() { "_" } else { "" },
            capability.name()
        );
        let exports = file.exports().map_err(|e| e.to_string())?;
        if !exports
            .iter()
            .any(|export| export.name() == marker.as_bytes())
        {
            return Err(format!(
                "{}: missing current ABI marker {marker}; rebuild the target capability library",
                path.display()
            ));
        }
    }
    Ok(())
}
