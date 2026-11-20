//! Tool invocations for wasm32 builds: clang for the guest runtime's bitcode (Dream's own
//! freestanding headers, no WASI sysroot) and `wasm-ld` for the final link.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::driver::wasm_opt::OptLevel;

/// True when clang/ld should emit colored diagnostics (we capture their output, so their own
/// TTY detection would otherwise strip colors).
fn tool_color() -> bool {
    crate::driver::ui::color_enabled()
}

/// Runs `cmd` capturing its output so failures can be reported as a styled, attributed error
/// instead of raw interleaved stderr. Returns `Err` with a message including the captured tool
/// output when the command fails. Callers pass `-fcolor-diagnostics` / `--color-diagnostics`
/// themselves when stderr is a TTY (we capture, so the tool's own TTY detection strips colors).
pub(crate) fn run_captured(cmd: &mut Command, what: &str) -> Result<(), String> {
    let _tool = tracing::info_span!("compile_tool", tool = what).entered();
    let out = cmd
        .output()
        .map_err(|e| format!("failed to spawn {what}: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let mut msg = format!("{what} failed");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    for captured in [stderr.trim(), stdout.trim()] {
        if !captured.is_empty() {
            msg.push('\n');
            msg.push_str(captured);
        }
    }
    Err(msg)
}

/// Appends a concrete fix under common toolchain failure patterns.
pub fn hint_for_failure(msg: &str) -> Option<&'static str> {
    if msg.contains("undefined symbol") || msg.contains("undefined reference") {
        Some("your Dream install may be incomplete or out of date — reinstall it")
    } else {
        None
    }
}

/// `wasm-ld` with the guest module's export/memory/stack flags; objects and `-o` not yet added.
pub(crate) fn wasm_ld_command(
    wasm_ld: &Path,
    threads: bool,
    opt: OptLevel,
) -> Result<Command, String> {
    let mut cmd = Command::new(wasm_ld);
    cmd.args([
        "--no-entry",
        "--allow-undefined",
        "--import-memory",
        "--export-memory",
        "--export-table",
        "--export=__stack_pointer",
        "--export=__tls_base",
        "--export=dream_malloc",
        "--export=dream_free",
        "--export=dream_heap_init",
        "--gc-sections",
        "--strip-debug",
    ]);
    if opt != OptLevel::O0 {
        cmd.arg("-O2");
    }
    cmd.arg(format!("-zstack-size={}", stack_size_bytes()?));
    if threads {
        cmd.args([
            "--export=__tls_size",
            "--export=__tls_align",
            "--export=__wasm_init_tls",
        ]);
        cmd.arg("--export=dream_publish");
        let max_bytes =
            u64::from(dream_mir::abi::MAX_MEMORY_PAGES) * u64::from(dream_mir::abi::WASM_PAGE_SIZE);
        cmd.arg("--shared-memory");
        cmd.arg(format!("--max-memory={max_bytes}"));
        // Clang's wasm32 default feature set is wider than atomics/bulk-memory. Restricting
        // `--features` to those two makes wasm-ld reject the rest (`sign-ext`, …).
        cmd.arg("--no-check-features");
    }
    if tool_color() {
        cmd.arg("--color-diagnostics");
    }
    Ok(cmd)
}

/// The runtime include directories every guest unit compiles against.
pub(crate) fn guest_include_dirs(root: &Path) -> Vec<PathBuf> {
    let inc_native = dream_mir::runtime::core_runtime_include_dir(root);
    vec![
        dream_mir::runtime::wasm32_runtime_include_dir(root),
        dream_mir::runtime::runtime_abi_include_dir(root),
        inc_native,
    ]
}

// Generated functions inherit the signature anchor's features; its cache and runtime caches
// must fingerprint the same policy as the C units.
pub(crate) const GUEST_FEATURES: &[&str] = &["-mbulk-memory", "-mmutable-globals", "-msimd128"];

pub(crate) struct GuestUnitOutput<'a> {
    pub stable_name: &'a str,
    pub bitcode: bool,
}

/// `source`'s path under the runtime tree, the name its output records. It is part of the unit's
/// cache key, so it must not depend on which other modules a runtime set links.
pub(crate) fn stable_unit_name(root: &Path, source: &Path) -> String {
    let relative = source.strip_prefix(root).unwrap_or(source);
    let parts: Vec<_> = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect();
    format!("rt/{}", parts.join("/"))
}

/// The clang invocation for one guest runtime unit (output and input not yet added). C
/// units become LLVM bitcode for the whole-program link; assembly units stay objects. `sysroot`
/// supplies the libc headers (`string.h`, …) the guest libc implements itself.
#[allow(clippy::too_many_arguments)]
pub(crate) fn unit_command(
    clang: &Path,
    sysroot: &Path,
    src: &Path,
    includes: &[&Path],
    extra_includes: &[PathBuf],
    defines: &[String],
    threads: bool,
    opt: OptLevel,
    output: GuestUnitOutput<'_>,
) -> Command {
    let is_asm = src.extension().and_then(|e| e.to_str()) == Some("s");
    let mut cmd = Command::new(clang);
    cmd.args(["--target=wasm32-unknown-unknown", "-nostdlib", "-c", "-g0"])
        .arg(format!("--sysroot={}", sysroot.display()));
    if is_asm {
        cmd.arg("-Wno-unused-command-line-argument");
    } else {
        cmd.args([
            opt.wasm_clang_opt_flag(),
            "-fno-ident",
            "-fno-exceptions",
            "-ffunction-sections",
            "-fdata-sections",
            "-frandom-seed=0",
            "-Wno-unused-value",
            "-DDREAM_WASM32",
            "-D__wasi__=1",
            "-D__wasip1__=1",
        ]);
        cmd.arg("-isystem")
            .arg(sysroot.join("include/wasm32-wasip1"));
        cmd.args(GUEST_FEATURES);
        if dream_mir::runtime::wasm32_unit_defines_libc(src) {
            cmd.arg("-fno-builtin");
        }
        if output.bitcode {
            cmd.arg("-emit-llvm");
        }
        cmd.arg(format!(
            "-ffile-prefix-map={}={}",
            src.display(),
            output.stable_name
        ));
        if threads {
            cmd.args(["-matomics", "-DDREAM_WASM32_THREADS"]);
        }
        for inc in includes {
            cmd.arg("-I").arg(inc);
        }
        for inc in extra_includes {
            cmd.arg("-I").arg(inc);
        }
        for d in defines {
            cmd.arg(format!("-D{d}"));
        }
    }
    if tool_color() {
        cmd.arg("-fcolor-diagnostics");
    }
    cmd
}

const MIN_STACK_SIZE: u64 = 64 * 1024;
const FALLBACK_STACK_SIZE: u64 = 16 * 1024 * 1024;

/// Guest call-stack size for the linked module: `DREAM_STACK_SIZE` (e.g. `32M`, `32MiB`, or a
/// plain byte count) when set, otherwise `[package.metadata.dream] stack-size`. wasm-ld's own
/// 64 KiB default would trap ordinary recursion, so a size is always chosen.
pub(crate) fn stack_size_bytes() -> Result<u64, String> {
    let size = match std::env::var("DREAM_STACK_SIZE") {
        Ok(v) => parse_size(&v)
            .filter(|&n| n >= MIN_STACK_SIZE)
            .ok_or_else(|| {
                format!("DREAM_STACK_SIZE={v:?} must be a size of at least 64KiB (e.g. 32M)")
            })?,
        Err(_) => option_env!("DREAM_DEFAULT_STACK_SIZE")
            .and_then(parse_size)
            .unwrap_or(FALLBACK_STACK_SIZE),
    };
    // wasm-ld requires a 16-byte aligned stack.
    Ok(size.next_multiple_of(16))
}

fn parse_size(s: &str) -> Option<u64> {
    let t = s.trim();
    let (digits, mult) = if let Some(n) = t.strip_suffix("GiB") {
        (n, 1024 * 1024 * 1024u64)
    } else if let Some(n) = t.strip_suffix("MiB") {
        (n, 1024 * 1024)
    } else if let Some(n) = t.strip_suffix("KiB") {
        (n, 1024)
    } else if let Some(n) = t.strip_suffix('G') {
        (n, 1024 * 1024 * 1024)
    } else if let Some(n) = t.strip_suffix('M') {
        (n, 1024 * 1024)
    } else if let Some(n) = t.strip_suffix('K') {
        (n, 1024)
    } else {
        (t, 1)
    };
    digits.trim().parse::<u64>().ok().map(|n| n * mult)
}
