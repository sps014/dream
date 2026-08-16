//! Invoke clang to compile LLVM IR + dream-rt into a native or wasm object/executable.

use super::options::{CodegenOptions, Lto, Sanitize};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub enum ClangError {
    Io(io::Error),
    Spawn(String),
    Failed { status: i32, stderr: String },
}

impl std::fmt::Display for ClangError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClangError::Io(e) => write!(f, "{}", e),
            ClangError::Spawn(s) => write!(f, "{}", s),
            ClangError::Failed { status, stderr } => {
                write!(f, "clang exited with status {}: {}", status, stderr)
            }
        }
    }
}

impl From<io::Error> for ClangError {
    fn from(e: io::Error) -> Self {
        ClangError::Io(e)
    }
}

fn clang_bin() -> String {
    std::env::var("DREAM_CLANG").unwrap_or_else(|_| "clang".to_string())
}

/// Compile `ir` with dream-rt C sources. Native: executable at `out`. Wasm: `out` should end in `.wasm`.
pub fn compile_ir(ir: &str, out: &Path, opts: &CodegenOptions) -> Result<PathBuf, ClangError> {
    let dir = out
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let ll = dir.join(format!(
        "{}.ll",
        out.file_stem().and_then(|s| s.to_str()).unwrap_or("dream")
    ));
    std::fs::write(&ll, ir)?;

    let clang = clang_bin();
    let mut cmd = Command::new(&clang);
    cmd.arg(format!("--target={}", opts.triple.as_str()));
    cmd.arg(format!("-O{}", opts.opt_level.min(3)));
    cmd.arg("-Wno-override-module");
    if opts.debug_info {
        cmd.arg("-g");
        cmd.arg("-gdwarf-4");
    }
    match opts.lto {
        Lto::None => {}
        Lto::Thin => {
            cmd.arg("-flto=thin");
        }
        Lto::Full => {
            cmd.arg("-flto");
        }
    }
    if opts.sanitize == Sanitize::Address && !opts.triple.is_wasm() {
        cmd.arg("-fsanitize=address");
        cmd.arg("-fno-omit-frame-pointer");
    }
    if opts.triple.is_wasm() {
        for feat in opts.mattr.split(',') {
            let f = feat.trim().trim_start_matches('+');
            if !f.is_empty() {
                cmd.arg(format!("-m{}", f));
            }
        }
        cmd.arg("-ffreestanding");
        cmd.arg("-nostdlib");
        cmd.arg("-Wl,--no-entry");
        cmd.arg("-Wl,--export-all");
        cmd.arg("-Wl,--allow-undefined");
    }
    if let Some(root) = &opts.sysroot {
        cmd.arg(format!("--sysroot={}", root));
    }
    cmd.arg("-I");
    cmd.arg(dream_rt::c_include_dir());
    if opts.triple.is_wasm() {
        for src in dream_rt::c_sources() {
            if src.ends_with("entry.c") {
                continue;
            }
            cmd.arg(src);
        }
        cmd.arg(&ll);
    } else {
        for src in dream_rt::c_sources() {
            if src.ends_with("dream_rt.c") || src.ends_with("dream_host.c") {
                continue;
            }
            cmd.arg(src);
        }
        cmd.arg(&ll);
        let archive = dream_rt::native_archive();
        if !archive.exists() {
            return Err(ClangError::Spawn(format!(
                "missing {} (rebuild dream-rt as staticlib)",
                archive.display()
            )));
        }
        cmd.arg(&archive);
        for arg in dream_rt::native_sys_libs() {
            cmd.arg(arg);
        }
        cmd.arg("-lm");
        cmd.arg("-lpthread");
    }
    cmd.arg("-o");
    cmd.arg(out);

    let output = cmd.output().map_err(|e| {
        ClangError::Spawn(format!(
            "failed to spawn clang ({}): {}. Install LLVM/clang or set DREAM_CLANG.",
            clang,
            e
        ))
    })?;
    if !output.status.success() {
        return Err(ClangError::Failed {
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(out.to_path_buf())
}
