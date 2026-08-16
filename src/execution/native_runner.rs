//! Run a clang-linked Dream executable (host triple). Stdout is the program's print stream.

use std::path::Path;
use std::process::Command;

pub fn execute_native(bin: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new(bin).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} exited with {}", bin.display(), status).into())
    }
}

pub fn execute_native_capturing(bin: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let out = Command::new(bin).output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!(
            "{} exited with {}: {}",
            bin.display(),
            out.status,
            err
        )
        .into())
    }
}
