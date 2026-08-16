//! Run a clang-linked Dream executable (host triple). Stdout is the program's print stream.

use std::path::Path;
use std::process::Command;

fn with_gpu_abi(mut cmd: Command, bin: &Path) -> Command {
    let abi = bin.with_extension("abi.json");
    if abi.exists() {
        cmd.env("DREAM_ABI_JSON", &abi);
    }
    cmd
}

pub fn execute_native(bin: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let status = with_gpu_abi(Command::new(bin), bin).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} exited with {}", bin.display(), status).into())
    }
}

pub fn execute_native_capturing(bin: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let out = with_gpu_abi(Command::new(bin), bin).output()?;
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
