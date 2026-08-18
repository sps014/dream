use dream::driver::compiler::{Compiler, Target};
use dream::driver::js_runtime::JsRuntimeTarget;
use dream::driver::wasm_opt::OptLevel;
use dream::execution::native_c::{compile_native_c, run_native_bin};
use dream::execution::wasm_runner::execute_wasm;
use dream_abi::attributes::CompileTargets;
use dream_sema::analyzer::CrateType;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

/// Returns a non-zero [`ExitCode`] on any failure (bad arguments, invalid path, compile error, or
/// run error) so CI pipelines and shell scripts can detect and react to failures. `--help`/`-h`
/// prints usage and exits successfully.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("aot") {
        return aot_command(&args);
    }
    let program = args
        .first()
        .map(String::as_str)
        .unwrap_or("dream")
        .to_string();

    let mut verbose = false;
    let mut run_after_compile = false;
    let mut run_tests = false;
    let mut test_filter: Option<String> = None;
    let mut release = false;
    let mut debug_info = false;
    let mut debug_adapter = false;
    let mut show_help = false;
    let mut file_name = None;
    let mut optimize: Option<OptLevel> = None;
    let mut want_runtime = false;
    let mut want_web = false;
    let mut want_node = false;
    let mut explicit_target: Option<CompileTargets> = None;
    let mut crate_type = CrateType::Bin;
    let mut crate_type_explicit = false;
    let mut native_c = false;

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-v" || arg == "--verbose" {
            verbose = true;
        } else if arg == "--release" {
            // Trimmed release build: uninstrumented allocator + structural WAT dead-function
            // elimination + wasm-opt at OptLevel::RELEASE_DEFAULT (-O3) unless -O overrides.
            // Default (no flag) keeps allocator probes and the full runtime.
            release = true;
        } else if arg == "-g" || arg == "--debug-info" {
            // Enable source-level debug-info: line hooks + a `.dbg.json` source map for the
            // interactive debugger. Off by default (zero overhead in normal builds). Combined with
            // `--release`, allocator instrumentation is still off, but WAT DCE stays disabled
            // because the debugger needs the full module.
            debug_info = true;
        } else if arg == "-h" || arg == "--help" {
            show_help = true;
        } else if arg == "run" {
            run_after_compile = true;
        } else if arg == "test" {
            run_tests = true;
        } else if arg == "--filter" {
            i += 1;
            let Some(val) = args.get(i) else {
                error!("--filter requires a substring");
                return ExitCode::FAILURE;
            };
            test_filter = Some(val.clone());
        } else if let Some(val) = arg.strip_prefix("--filter=") {
            test_filter = Some(val.to_string());
        } else if arg == "debug-adapter" {
            // Speak the Debug Adapter Protocol over stdio for the given source file (used by editor
            // debug clients such as the VS Code extension). Implies debug-info.
            debug_adapter = true;
            debug_info = true;
        } else if arg == "--runtime" {
            want_runtime = true;
        } else if arg == "--web" {
            want_web = true;
        } else if arg == "--node" {
            want_node = true;
        } else if arg == "--target" {
            i += 1;
            let Some(val) = args.get(i) else {
                error!("--target requires native, node, or web");
                return ExitCode::FAILURE;
            };
            if explicit_target.is_some() {
                error!("--target may only be specified once");
                return ExitCode::FAILURE;
            }
            explicit_target = Some(match val.as_str() {
                "native" => CompileTargets::native_only(),
                "node" => CompileTargets {
                    native: false,
                    node: true,
                    web: false,
                },
                "web" => CompileTargets {
                    native: false,
                    node: false,
                    web: true,
                },
                other => {
                    error!(
                        "unknown --target '{}': expected native, node, or web",
                        other
                    );
                    return ExitCode::FAILURE;
                }
            });
        } else if let Some(val) = arg.strip_prefix("--target=") {
            if explicit_target.is_some() {
                error!("--target may only be specified once");
                return ExitCode::FAILURE;
            }
            explicit_target = Some(match val {
                "native" => CompileTargets::native_only(),
                "node" => CompileTargets {
                    native: false,
                    node: true,
                    web: false,
                },
                "web" => CompileTargets {
                    native: false,
                    node: false,
                    web: true,
                },
                other => {
                    error!(
                        "unknown --target '{}': expected native, node, or web",
                        other
                    );
                    return ExitCode::FAILURE;
                }
            });
        } else if arg == "--native-c" || arg == "--backend=c" {
            native_c = true;
        } else if arg == "--backend=wasm" {
            native_c = false;
        } else if arg == "--backend" {
            i += 1;
            let Some(val) = args.get(i) else {
                error!("--backend requires wasm or c");
                return ExitCode::FAILURE;
            };
            match val.as_str() {
                "wasm" => native_c = false,
                "c" => native_c = true,
                other => {
                    error!("unknown --backend '{}': expected wasm or c", other);
                    return ExitCode::FAILURE;
                }
            }
        } else if arg == "--crate-type" {
            i += 1;
            let Some(val) = args.get(i) else {
                error!("--crate-type requires lib or bin");
                return ExitCode::FAILURE;
            };
            match val.as_str() {
                "lib" => crate_type = CrateType::Lib,
                "bin" => crate_type = CrateType::Bin,
                other => {
                    error!("unknown --crate-type '{}': expected lib or bin", other);
                    return ExitCode::FAILURE;
                }
            }
            crate_type_explicit = true;
        } else if let Some(val) = arg.strip_prefix("--crate-type=") {
            match val {
                "lib" => crate_type = CrateType::Lib,
                "bin" => crate_type = CrateType::Bin,
                other => {
                    error!("unknown --crate-type '{}': expected lib or bin", other);
                    return ExitCode::FAILURE;
                }
            }
            crate_type_explicit = true;
        } else if arg == "-O" || arg == "--optimize" {
            // No level given: default to `-Os` (optimize for size), matching the "smaller binary"
            // intent most users reach for this flag with. Also overrides `--release`'s default.
            optimize = Some(OptLevel::Size);
        } else if let Some(level_str) = arg.strip_prefix("--optimize=") {
            match level_str.parse::<OptLevel>() {
                Ok(level) => optimize = Some(level),
                Err(e) => {
                    error!("{}", e);
                    return ExitCode::FAILURE;
                }
            }
        } else if let Some(level_str) = arg.strip_prefix("-O") {
            let level_str = level_str.strip_prefix('=').unwrap_or(level_str);
            match level_str.parse::<OptLevel>() {
                Ok(level) => optimize = Some(level),
                Err(e) => {
                    error!("{}", e);
                    return ExitCode::FAILURE;
                }
            }
        } else if !arg.starts_with('-') {
            file_name = Some(arg);
        }
        i += 1;
    }
    let _ = crate_type_explicit;

    if !native_c && (release || optimize.is_some()) && !cfg!(feature = "wasm-opt") {
        error!(
            "--release / -O/--optimize requires the compiler to be built with the `wasm-opt` feature \
             (enabled by default); this build was compiled without it"
        );
        return ExitCode::FAILURE;
    }

    let mut runtimes = Vec::new();
    if want_web {
        runtimes.push(JsRuntimeTarget::Web);
    }
    if want_node {
        runtimes.push(JsRuntimeTarget::Node);
    }

    if want_runtime && runtimes.is_empty() {
        error!("--runtime requires --web and/or --node");
        print_usage(&program);
        return ExitCode::FAILURE;
    }
    if !runtimes.is_empty() && !want_runtime {
        error!("--web / --node require --runtime");
        print_usage(&program);
        return ExitCode::FAILURE;
    }

    if native_c && debug_adapter {
        error!("debug-adapter requires the wasm backend (omit --native-c)");
        return ExitCode::FAILURE;
    }
    if native_c && !runtimes.is_empty() {
        error!("--runtime --web/--node requires the wasm backend (omit --native-c)");
        return ExitCode::FAILURE;
    }

    let compile_targets = explicit_target.unwrap_or_else(|| {
        if runtimes.is_empty() {
            CompileTargets::native_only()
        } else {
            CompileTargets {
                native: false,
                node: want_node,
                web: want_web,
            }
        }
    });

    // Route logs to stderr so they never corrupt stdout — critical in `debug-adapter` mode, where
    // stdout carries the framed DAP protocol stream (and harmless/conventional for other modes).
    let subscriber = FmtSubscriber::builder()
        .with_max_level(if verbose { Level::INFO } else { Level::WARN })
        .without_time()
        .with_target(false)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    if show_help {
        print_usage(&program);
        return ExitCode::SUCCESS;
    }

    if run_tests {
        if run_after_compile || debug_adapter {
            error!("'test' cannot be combined with 'run' or 'debug-adapter'");
            return ExitCode::FAILURE;
        }
        let path = match file_name {
            Some(name) => PathBuf::from(name),
            None => {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let tests = cwd.join("tests");
                if tests.is_dir() {
                    tests
                } else {
                    error!("Expected a .dream file or tests/ directory (or run from a project with tests/)");
                    print_usage(&program);
                    return ExitCode::FAILURE;
                }
            }
        };
        let opts = dream::driver::test::TestOptions {
            release,
            optimize,
            native_c,
            filter: test_filter,
            verbose,
        };
        return match dream::driver::test::run_tests(&path, &opts) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                error!("{}", e);
                ExitCode::FAILURE
            }
        };
    }

    let file_name = match file_name {
        Some(name) => name,
        None => {
            error!("Expected a source file (*.dream) as argument");
            print_usage(&program);
            return ExitCode::FAILURE;
        }
    };

    info!("Dream Compiler Tools");
    info!("========================");
    info!("Compiling file: {}", file_name);

    // `with_release` installs RELEASE_DEFAULT wasm-opt; an explicit `-O` overrides. Do not call
    // `with_optimize(None)` after release — that would clear the default.
    // Always emit `.abi.json`: JS hosts need imports/exports, and native `run` / `debug-adapter`
    // load `abi.gpu` for `@compute` / shader metadata.
    let emit_abi = true;
    let mut compiler = Compiler::new(if native_c {
        Target::NativeC
    } else {
        Target::Wasm
    })
    .with_release(release)
    .with_debug_info(debug_info)
    .with_runtimes(runtimes)
    .with_compile_targets(compile_targets)
    .with_emit_abi(emit_abi)
    .with_crate_type(crate_type)
    .with_emit_cwasm(release && compile_targets.native && !native_c);
    if let Some(level) = optimize {
        compiler = compiler.with_optimize(Some(level));
    }
    let out_path = match get_path_from_file_path(file_name, release, native_c) {
        Some(path) => path,
        None => {
            error!("Invalid source file path: {}", file_name);
            return ExitCode::FAILURE;
        }
    };

    if let Some(parent) = Path::new(&out_path).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                error!(
                    "could not create output directory {}: {}",
                    parent.display(),
                    e
                );
                return ExitCode::FAILURE;
            }
        }
    }

    match compiler.compile(file_name, &out_path) {
        Ok(_) => {
            info!("Compilation successful");

            if debug_adapter {
                // Hand control to the Debug Adapter Protocol server, which loads the just-emitted
                // `.wat` + `.dbg.json` and drives execution under the debugger over stdio.
                if let Err(e) = dream::execution::debugger::run_debug_adapter(&out_path) {
                    error!("Debug adapter failed: {}", e);
                    return ExitCode::FAILURE;
                }
                return ExitCode::SUCCESS;
            }

            if native_c {
                let cc_opt = OptLevel::from_cli(release, optimize);
                match compile_native_c(std::path::Path::new(&out_path), cc_opt) {
                    Ok(bin) => {
                        info!("created file: {}", bin.display());
                        if run_after_compile {
                            info!("Executing native C...");
                            if let Err(e) = run_native_bin(&bin, &out_path) {
                                error!("Execution failed: {}", e);
                                return ExitCode::FAILURE;
                            }
                        }
                    }
                    Err(e) => {
                        error!("cc failed: {}", e);
                        return ExitCode::FAILURE;
                    }
                }
            } else if run_after_compile {
                info!("Executing via Wasmtime...");
                if let Err(e) = execute_wasm(&out_path) {
                    error!("Execution failed: {}", e);
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            error!("Compilation failed: {}", e.to_string());
            ExitCode::FAILURE
        }
    }
}

/// Prints CLI usage to stderr via the tracing subscriber's error channel.
fn print_usage(program: &str) {
    error!(
        "Usage: {} [-v|--verbose] [--release] [-g|--debug-info] [-O|--optimize[=LEVEL]] [--crate-type lib|bin] [--backend wasm|c] [--native-c] [--target native|node|web] [--runtime --web|--node] [--filter SUBSTR] [run|test|debug-adapter|aot] <file|dir>",
        program
    );
    error!("  -v, --verbose         Print progress information");
    error!(
        "  --release             Trimmed build; wasm-opt / cc default -O3 (-Os with --web); native wasm also emits .cwasm"
    );
    error!(
        "  -g, --debug-info      Emit source-level debug info (line hooks + .dbg.json source map)"
    );
    error!(
        "  -O, --optimize[=LVL]  wasm-opt and cc level (LVL: 0-4, s, z; default: s); overrides --release"
    );
    error!("  --backend wasm|c     Codegen backend (default: wasm / Wasmtime)");
    error!("  --native-c           Same as --backend c. Compiles with cc to .bin; `run` execs it");
    error!(
        "  --target native|node|web  Compile-time runtime target for availability checks (default: native)"
    );
    error!(
        "  --runtime             Emit tree-shaken *.(web|node).runtime.js (requires --web and/or --node)"
    );
    error!("  --web                 With --runtime: browser-targeted *.web.runtime.js");
    error!("  --node                With --runtime: Node-targeted *.node.runtime.js");
    error!(
        "  --filter SUBSTR       With `test`: only run @test functions whose names contain SUBSTR"
    );
    error!("  -h, --help            Show this help message");
    error!("  run                   Execute the compiled module after a successful build");
    error!("  test                  Discover and run @test functions in a file or directory");
    error!("  debug-adapter         Run the Debug Adapter Protocol server over stdio (implies -g)");
    error!("  aot <in.wasm> [out.cwasm] [--target TRIPLE]  Cranelift-precompile wasm for Wasmtime");
    error!(r"Example: {} run src/sample/test_arrays.dream", program);
    error!(r"Example: {} test tests/", program);
    error!(r"Example: {} --filter adds test tests/math.dream", program);
    error!(
        r"Example: {} --release run src/sample/test_arrays.dream",
        program
    );
    error!(
        r"Example: {} --runtime --web sample/interop/js.dream",
        program
    );
    error!(
        r"Example: {} --runtime --node sample/interop/js.dream",
        program
    );
    error!(
        r"Example: {} --runtime --web --node sample/interop/js.dream",
        program
    );
}

/// Walk upward from a file's directory looking for `dream.toml`.
fn find_project_root(file_path: &Path) -> Option<PathBuf> {
    let mut dir = file_path.parent().map(Path::to_path_buf)?;
    loop {
        if dir.join("dream.toml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Derives the output `.wat` path.
///
/// When a `dream.toml` encloses the source file, artifacts go under
/// `target/debug/` or `target/release/` at the project root. Otherwise they sit beside the source.
fn get_path_from_file_path(file_path: &str, release: bool, native_c: bool) -> Option<String> {
    let path = Path::new(file_path);
    let file_stem = path.file_stem()?.to_str()?;
    let out_dir = if let Some(root) = find_project_root(path) {
        let profile = if release { "release" } else { "debug" };
        root.join("target").join(profile)
    } else {
        path.parent().unwrap_or_else(|| Path::new("")).to_path_buf()
    };
    let ext = if native_c { "c" } else { "wat" };
    let result = out_dir.join(format!("{file_stem}.{ext}"));
    Some(result.to_str()?.to_string())
}

/// `dream aot <in.wasm> [out.cwasm] [--target <rustc-triple>]` — Cranelift-precompile for Wasmtime.
fn aot_command(args: &[String]) -> ExitCode {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::WARN)
        .without_time()
        .with_target(false)
        .with_writer(std::io::stderr)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    let mut target: Option<String> = None;
    let mut positionals: Vec<&str> = Vec::new();
    let mut i = 2;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--target" {
            i += 1;
            let Some(val) = args.get(i) else {
                error!("aot --target requires a rustc/Wasmtime triple");
                return ExitCode::FAILURE;
            };
            target = Some(val.clone());
        } else if let Some(val) = arg.strip_prefix("--target=") {
            target = Some(val.to_string());
        } else if arg == "-h" || arg == "--help" {
            error!("Usage: dream aot <in.wasm> [out.cwasm] [--target TRIPLE]");
            return ExitCode::SUCCESS;
        } else if arg.starts_with('-') {
            error!("unknown aot flag '{}'", arg);
            return ExitCode::FAILURE;
        } else {
            positionals.push(arg.as_str());
        }
        i += 1;
    }

    let Some(wasm_path) = positionals.first() else {
        error!("Usage: dream aot <in.wasm> [out.cwasm] [--target TRIPLE]");
        return ExitCode::FAILURE;
    };
    let wasm_path = PathBuf::from(wasm_path);
    let cwasm_path = match positionals.get(1) {
        Some(p) => PathBuf::from(*p),
        None => wasm_path.with_extension("cwasm"),
    };

    match dream::execution::cwasm::write_cwasm_file(&wasm_path, &cwasm_path, target.as_deref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{}", e);
            ExitCode::FAILURE
        }
    }
}
