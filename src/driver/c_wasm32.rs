//! Compile C-backend wasm32 output with wasi-sdk clang / wasm-ld.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::SystemTime;

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
    if msg.contains("not found") && msg.contains("clang") {
        Some("run `dreamer toolchain install wasi-sdk` to get the WebAssembly toolchain")
    } else if msg.contains("undefined symbol") || msg.contains("undefined reference") {
        Some(
            "your installed toolchain may be out of date — run `dreamer toolchain install` \
             to refresh it",
        )
    } else {
        None
    }
}

pub fn wasi_clang() -> Option<PathBuf> {
    if let Ok(sdk) = std::env::var("WASI_SDK_PATH") {
        if !sdk.is_empty() {
            let clang = PathBuf::from(sdk).join("bin").join(clang_name());
            if clang.is_file() && clang.parent().is_some_and(is_wasi_bin_dir) {
                return Some(clang);
            }
        }
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let root = PathBuf::from(home).join(".dream").join("toolchains");
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&root) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir()
                && p.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|n| n.starts_with("wasi-sdk-"))
            {
                dirs.push(p);
            }
        }
    }
    dirs.sort();
    dirs.into_iter()
        .rev()
        .map(|d| d.join("bin").join(clang_name()))
        .find(|p| p.is_file() && p.parent().is_some_and(is_wasi_bin_dir))
}

fn is_wasi_bin_dir(dir: &Path) -> bool {
    dir.join(if cfg!(windows) {
        "wasm-ld.exe"
    } else {
        "wasm-ld"
    })
    .is_file()
        && dir.join("clang.cfg").is_file()
}

fn clang_name() -> &'static str {
    if cfg!(windows) {
        "clang.exe"
    } else {
        "clang"
    }
}

/// Cache root for precompiled wasm32 runtime objects, keyed by opt level + threads
/// (`~/.dream/cache/wasm32-rt/<subdir>`). `None` when there is no HOME to cache under.
fn rt_cache_dir(opt: OptLevel, threads: bool, need: dream_mir::runtime::RuntimeNeed) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let sub = format!(
        "{}{}-need{:x}",
        opt.native_rt_subdir(),
        if threads { "-threads" } else { "" },
        need.bits()
    );
    Some(
        PathBuf::from(home)
            .join(".dream")
            .join("cache")
            .join("wasm32-rt")
            .join(sub),
    )
}

/// Newest source mtime over the always-on units, their headers, and the linked-library
/// catalog sources.
fn runtime_input_mtimes(
    sources: &[PathBuf],
    includes: &[&Path],
    linked: &[dream_mir::runtime::Wasm32LinkedUnit],
) -> Option<SystemTime> {
    let mut paths: Vec<PathBuf> = sources.to_vec();
    for u in linked {
        paths.push(u.path.clone());
    }
    newest_mtime(&paths, includes)
}

fn newest_mtime(paths: &[PathBuf], dirs: &[&Path]) -> Option<SystemTime> {
    let mut newest: Option<SystemTime> = None;
    let mut consider = |p: &Path| {
        if let Ok(m) = std::fs::metadata(p) {
            if let Ok(t) = m.modified() {
                if newest.is_none_or(|n| t > n) {
                    newest = Some(t);
                }
            }
        }
    };
    for p in paths {
        consider(p);
    }
    // Headers too: a runtime-unit rebuild must trigger when only an include changed.
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Ok(sub) = std::fs::read_dir(&p) {
                    for e2 in sub.flatten() {
                        consider(&e2.path());
                    }
                }
            } else {
                consider(&p);
            }
        }
    }
    newest
}

fn compile_runtime_units(
    clang: &Path,
    sources: &[PathBuf],
    includes: &[&Path],
    threads: bool,
    opt: OptLevel,
    obj_name: impl Fn(usize) -> String,
    obj_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut objs = Vec::new();
    for (i, src) in sources.iter().enumerate() {
        let obj = obj_dir.join(obj_name(i));
        compile_unit(
            clang,
            src,
            &obj,
            includes,
            threads,
            opt,
            &format!("rt{i}.c"),
        )?;
        objs.push(obj);
    }
    Ok(objs)
}

/// Compiled wasm32 runtime units, cached across compiles (mirrors the native `runtime_archive`):
/// mtime staleness over the runtime sources *and* their headers, a cross-process flock so parallel
/// `dream` invocations cannot race the rebuild, and a per-process mutex for in-process parallelism.
/// Falls back to compiling next to the output when no HOME is available.
#[allow(clippy::too_many_arguments)]
fn cached_runtime_objects(
    clang: &Path,
    includes: &[&Path],
    threads: bool,
    need: dream_mir::runtime::RuntimeNeed,
    opt: OptLevel,
    stem: &str,
    obj_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    static LOCK: Mutex<()> = Mutex::new(());
    let sources = dream_mir::runtime::wasm32_runtime_c_files();
    let linked = dream_mir::runtime::wasm32_linked_units(need);
    let fallback = || {
        let mut objs = compile_runtime_units(clang, &sources, includes, threads, opt, |i| {
            format!("{stem}_rt_{i}.o")
        }, obj_dir)?;
        objs.extend(compile_linked_units(clang, &linked, includes, threads, opt, |i| {
            format!("{stem}_lib_{i}.o")
        }, obj_dir)?);
        Ok(objs)
    };
    let Some(cache) = rt_cache_dir(opt, threads, need) else {
        return fallback();
    };
    if let Err(e) = std::fs::create_dir_all(&cache) {
        eprintln!("warning: wasm32 runtime cache unavailable ({}); compiling in place", e);
        return fallback();
    }
    let _ = &linked;
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(cache.join(".lock"))
        .map_err(|e| e.to_string())?;
    lock_file.lock().map_err(|e| e.to_string())?;
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // Core units are `{i}.o`; catalog-linked units (regex/PCRE2) are `lib{i}.o`.
    let mut objs: Vec<PathBuf> = (0..sources.len())
        .map(|i| cache.join(format!("{i}.o")))
        .collect();
    objs.extend(
        (0..linked.len()).map(|i| cache.join(format!("lib{i}.o"))),
    );
    let stamp = cache.join(".stamp");
    let stale = match (
        runtime_input_mtimes(&sources, includes, &linked),
        std::fs::metadata(&stamp).ok().and_then(|m| m.modified().ok()),
    ) {
        (Some(src), Some(st)) => src > st,
        _ => true,
    };
    if !stale && objs.iter().all(|o| o.is_file()) {
        return Ok(objs);
    }
    let mut built = compile_runtime_units(clang, &sources, includes, threads, opt, |i| {
        format!("{i}.o")
    }, &cache)?;
    built.extend(compile_linked_units(clang, &linked, includes, threads, opt, |i| {
        format!("lib{i}.o")
    }, &cache)?);
    std::fs::write(&stamp, b"ok").map_err(|e| e.to_string())?;
    debug_assert_eq!(built.len(), objs.len());
    Ok(built)
}

/// Compile the catalog's linked-library units (regex.c + vendored PCRE2) with their
/// per-module defines / include dirs. `obj_name` names objects uniquely inside `obj_dir`.
fn compile_linked_units(
    clang: &Path,
    linked: &[dream_mir::runtime::Wasm32LinkedUnit],
    base_includes: &[&Path],
    threads: bool,
    opt: OptLevel,
    obj_name: impl Fn(usize) -> String,
    obj_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut objs = Vec::new();
    for (i, unit) in linked.iter().enumerate() {
        let obj = obj_dir.join(obj_name(i));
        let is_asm = unit.path.extension().and_then(|e| e.to_str()) == Some("s");
        let mut cmd = Command::new(clang);
        cmd.args(["--target=wasm32-wasip1", "-nostdlib", "-c", "-g0"]);
        if is_asm {
            cmd.arg("-Wno-unused-command-line-argument");
        } else {
            cmd.args([
                opt.wasm_clang_opt_flag(),
                "-fno-ident",
                "-fno-exceptions",
                // PCRE2 ships its own memmove-ish helpers and feature detection; keep the
                // same no-builtin discipline as the other guest units.
                "-fno-builtin",
                "-mbulk-memory",
                "-mmutable-globals",
                "-ffunction-sections",
                "-fdata-sections",
                "-frandom-seed=0",
                "-Wno-unused-value",
                "-DDREAM_WASM32",
            ]);
            if opt != OptLevel::O0 {
                cmd.arg("-flto");
            }
            cmd.arg(format!("-ffile-prefix-map={}=lib{i}.c", unit.path.display()));
            if threads {
                cmd.args(["-matomics", "-DDREAM_WASM32_THREADS"]);
            }
            for inc in base_includes {
                cmd.arg("-I").arg(inc);
            }
            for inc in &unit.include_dirs {
                cmd.arg("-I").arg(inc);
            }
            for d in &unit.defines {
                cmd.arg(format!("-D{d}"));
            }
        }
        if tool_color() {
            cmd.arg("-fcolor-diagnostics");
        }
        run_captured(
            cmd.arg("-o").arg(&obj).arg(&unit.path),
            &format!("clang ({})", unit.path.display()),
        )?;
        objs.push(obj);
    }
    Ok(objs)
}

pub fn compile_c_to_wasm32(
    c_path: &Path,
    wasm_path: &Path,
    threads: bool,
    need: dream_mir::runtime::RuntimeNeed,
    opt: OptLevel,
) -> Result<(), String> {
    let clang = wasi_clang().ok_or_else(|| {
        "wasi-sdk clang not found; run `dreamer toolchain install wasi-sdk`".to_string()
    })?;
    let wasm_ld = clang.parent().unwrap().join(if cfg!(windows) {
        "wasm-ld.exe"
    } else {
        "wasm-ld"
    });
    if !wasm_ld.is_file() {
        return Err(format!("wasm-ld missing next to {}", clang.display()));
    }
    let inc_abi = dream_mir::runtime::runtime_abi_include_dir();
    let inc_wasm = dream_mir::runtime::wasm32_runtime_include_dir();
    let inc_native = dream_mir::runtime::native_runtime_include_dir();
    let inc_native_parent = inc_native.parent().unwrap_or(&inc_native);
    let includes = [inc_wasm.as_path(), inc_abi.as_path(), inc_native_parent];
    let obj_dir = wasm_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = wasm_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("dream");
    let mut objs: Vec<PathBuf> = Vec::new();
    let guest_o = obj_dir.join(format!("{stem}_guest.o"));
    compile_unit(
        &clang,
        c_path,
        &guest_o,
        &includes,
        threads,
        opt,
        "guest.c",
    )?;
    objs.push(guest_o);
    objs.extend(cached_runtime_objects(
        &clang, &includes, threads, need, opt, stem, obj_dir,
    )?);
    let mut cmd = Command::new(&wasm_ld);
    cmd.args([
        "--no-entry",
        "--allow-undefined",
        "--import-memory",
        "--export-memory",
        "--export-table",
        "--export=__stack_pointer",
        "--export=__tls_base",
        "--export=dream_malloc",
        "--gc-sections",
        "--strip-debug",
    ]);
    if opt != OptLevel::O0 {
        // Optimize at link time too (dead-section stripping already via --gc-sections; this
        // also drives the LTO pipeline when units were compiled with -flto).
        cmd.arg("-O2");
    }
    if let Some(stack) = stack_size_bytes() {
        cmd.arg(format!("-zstack-size={stack}"));
    }
    if threads {
        let max_bytes = u64::from(dream_mir::abi::MAX_MEMORY_PAGES)
            * u64::from(dream_mir::abi::WASM_PAGE_SIZE);
        cmd.arg("--shared-memory");
        cmd.arg(format!("--max-memory={max_bytes}"));
        // Clang's wasm32 default feature set is wider than atomics/bulk-memory. Restricting
        // `--features` to those two makes wasm-ld reject the rest (`sign-ext`, …).
        cmd.arg("--no-check-features");
    }
    if tool_color() {
        cmd.arg("--color-diagnostics");
    }
    cmd.arg("-o").arg(wasm_path);
    for o in &objs {
        cmd.arg(o);
    }
    run_captured(&mut cmd, "wasm-ld")
}

fn compile_unit(
    clang: &Path,
    src: &Path,
    obj: &Path,
    includes: &[&Path],
    threads: bool,
    opt: OptLevel,
    stable_name: &str,
) -> Result<(), String> {
    let is_asm = src.extension().and_then(|e| e.to_str()) == Some("s");
    let mut cmd = Command::new(clang);
    cmd.args(["--target=wasm32-wasip1", "-nostdlib", "-c", "-g0"]);
    if is_asm {
        cmd.arg("-Wno-unused-command-line-argument");
    } else {
        cmd.args([
            opt.wasm_clang_opt_flag(),
            "-fno-ident",
            "-fno-exceptions",
            "-fno-builtin",
            // Bulk-memory + mutable-globals unconditionally: the guest libc lowers
            // memcpy/memset to `memory.copy`/`memory.fill`, and wasm-opt already assumes
            // these features in every emitted module.
            "-mbulk-memory",
            "-mmutable-globals",
            "-ffunction-sections",
            "-fdata-sections",
            "-frandom-seed=0",
            "-Wno-unused-value",
            "-DDREAM_WASM32",
        ]);
        if opt != OptLevel::O0 {
            // LTO lets clang inline runtime helpers (`dream_free`, memops, typed release
            // glue) into guest code across translation units at link time.
            cmd.arg("-flto");
        }
        cmd.arg(format!(
            "-ffile-prefix-map={}={}",
            src.display(),
            stable_name
        ));
        if threads {
            cmd.args(["-matomics", "-DDREAM_WASM32_THREADS"]);
        }
        for inc in includes {
            cmd.arg("-I").arg(inc);
        }
    }
    if tool_color() {
        cmd.arg("-fcolor-diagnostics");
    }
    run_captured(
        cmd.arg("-o").arg(obj).arg(src),
        &format!("clang ({})", src.display()),
    )
}

/// Guest call-stack size for the linked module, from `DREAM_STACK_SIZE` (e.g. `32M`, `32MiB`,
/// or a plain byte count). `None` leaves wasm-ld's default.
fn stack_size_bytes() -> Option<u64> {
    let v = std::env::var("DREAM_STACK_SIZE").ok()?;
    parse_size(&v)
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
