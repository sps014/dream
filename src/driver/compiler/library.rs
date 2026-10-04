use super::*;

pub(super) fn relativize_sources(
    mir: &mut dream_mir::Mir,
    entry: &str,
) -> Result<(), CompileError> {
    let entry = Path::new(entry).canonicalize()?;
    let directory = entry.parent().unwrap_or_else(|| Path::new("."));
    let root = crate::driver::project_manifest::find_project_root_from(directory)
        .unwrap_or_else(|| directory.to_path_buf());
    let package = if root
        .join(crate::driver::project_manifest::MANIFEST_FILE_NAME)
        .is_file()
    {
        crate::driver::project_manifest::ProjectManifest::load(&root)
            .map_err(CompileError::Manifest)?
            .package_name
    } else {
        None
    }
    .unwrap_or_else(|| {
        root.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    });
    if Path::new(&package).components().count() != 1 || Path::new(&package).is_absolute() {
        return Err(CompileError::Manifest(
            "library package name must be one relative path component".into(),
        ));
    }
    for f in mir.functions.iter_mut().chain(&mut mir.polls) {
        if let Some(file) = &mut f.file {
            if dream_stdlib::is_std_source(file) {
                continue;
            }
            let path = Path::new(file).canonicalize()?;
            let relative = path.strip_prefix(&root).map_err(|_| {
                CompileError::Manifest(format!(
                    "library source {} is outside package root {}",
                    path.display(),
                    root.display()
                ))
            })?;
            *file = Path::new(&package)
                .join(relative)
                .to_string_lossy()
                .replace('\\', "/");
        }
    }
    Ok(())
}
