//! Ownership of capability companions in a shared native output directory.

use dream_abi::{host_capability::HostCapability, target::TargetSpec};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Deserialize, Serialize)]
struct Manifest {
    target: String,
    capabilities: Vec<String>,
}

fn libraries(manifest: &Manifest, spec: &TargetSpec) -> Result<BTreeSet<String>, String> {
    let mut names = BTreeSet::new();
    if manifest.target != spec.triple.to_string() {
        return Ok(names);
    }
    for name in &manifest.capabilities {
        let capability = HostCapability::ALL
            .iter()
            .find(|c| c.name() == name)
            .ok_or_else(|| format!("unknown capability in native runtime manifest: {name}"))?;
        names.insert(capability.library_name(spec));
        names.extend(
            capability
                .companion_library_names(spec)
                .iter()
                .map(|n| n.to_string()),
        );
    }
    Ok(names)
}

/// Remove only companions recorded by this executable and unreferenced by its siblings.
pub(crate) fn update(
    bin: &Path,
    capabilities: &[HostCapability],
    spec: &TargetSpec,
) -> Result<(), String> {
    let path = bin.with_extension("native-runtime.json");
    let directory = bin.parent().unwrap_or_else(|| Path::new("."));
    let current = Manifest {
        target: spec.triple.to_string(),
        capabilities: capabilities.iter().map(|c| c.name().to_string()).collect(),
    };
    let old = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<Manifest>(&bytes)
            .map_err(|e| format!("{}: {e}", path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Manifest {
            target: current.target.clone(),
            capabilities: Vec::new(),
        },
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let mut keep = libraries(&current, spec)?;
    for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?.path();
        if entry == path
            || !entry
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".native-runtime.json"))
        {
            continue;
        }
        let bytes = std::fs::read(&entry).map_err(|e| e.to_string())?;
        let other = serde_json::from_slice::<Manifest>(&bytes)
            .map_err(|e| format!("{}: {e}", entry.display()))?;
        keep.extend(libraries(&other, spec)?);
    }
    for name in libraries(&old, spec)?.difference(&keep) {
        match std::fs::remove_file(directory.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("removing unused runtime {name}: {error}")),
        }
    }
    if capabilities.is_empty() {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    } else {
        let bytes = serde_json::to_vec_pretty(&current).map_err(|e| e.to_string())?;
        std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_only_owned_companions_after_their_last_application_drops_them() {
        let temp = tempfile::tempdir().unwrap();
        let spec = TargetSpec::host();
        let library = temp.path().join(HostCapability::Audio.library_name(&spec));
        let unrelated = temp.path().join("user.dat");
        std::fs::write(&library, b"audio").unwrap();
        std::fs::write(&unrelated, b"user").unwrap();
        let first = temp.path().join("first.bin");
        let second = temp.path().join("second.bin");
        update(&first, &[], &spec).unwrap();
        assert!(library.exists());
        update(&first, &[HostCapability::Audio], &spec).unwrap();
        update(&second, &[HostCapability::Audio], &spec).unwrap();
        update(&first, &[], &spec).unwrap();
        assert!(library.exists());
        update(&second, &[], &spec).unwrap();
        assert!(!library.exists());
        assert!(unrelated.exists());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}
