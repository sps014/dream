use std::fs;
use std::io::Error;
use std::path::{Path, PathBuf};

use crate::driver::abi::LiveImport;

pub(super) fn emit(
    output: &Path,
    imports: &[LiveImport],
    compress: bool,
) -> Result<Vec<PathBuf>, Error> {
    let mut written = Vec::new();
    for (stem, field, bytes) in [(
        "compression",
        "dataCompression",
        include_bytes!("../../../runtime/dream-compression.wasm").as_slice(),
    )] {
        let extension = format!("{stem}.wasm");
        let path = output.with_extension(&extension);
        if imports
            .iter()
            .any(|(module, name)| module == "Dream" && name == field)
        {
            fs::write(&path, bytes)?;
            written.push(path.clone());
            if compress {
                written.extend(
                    crate::driver::compress::write_precompressed(&path)
                        .into_iter()
                        .map(|(path, _)| path),
                );
            }
        } else {
            for suffix in [
                extension.clone(),
                format!("{extension}.gz"),
                format!("{extension}.br"),
            ] {
                match fs::remove_file(output.with_extension(suffix)) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
        }
    }
    Ok(written)
}
