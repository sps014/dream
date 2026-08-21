use dream::driver::compiler::{Compiler, Target};
use dream::driver::js_runtime::JsRuntimeTarget;
use dream::driver::wasm_opt::OptLevel;
use dream::execution::native_c::{compile_native_c, run_native_bin};
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
    let mut native_c = true;
    let mut out_override: Option<String> = None;
    let mut program_args: Vec<String> = Vec::new();
    let mut after_program_args = false;

    // Install before any `error!()` so bad flags are not a silent exit 1. Route to stderr so
    // `debug-adapter` stdout stays a clean DAP stream.
    let verbose_early = args.iter().any(|a| a == "-v" || a == "--verbose");
    let subscriber = FmtSubscriber::builder()
        .with_max_level(if verbose_early {
            Level::INFO
        } else {
            Level::WARN
        })
        .without_time()
        .with_target(false)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if after_program_args {
            program_args.push(arg.clone());
            i += 1;
            continue;
        }
        if arg == "--" {
            after_program_args = true;
            i += 1;
            continue;
        }
        if arg == "-v" || arg == "--verbose" {
            verbose = true;
        } else if arg == "--release" {
            // Trimmed release build: uninstrumented allocator + structural WAT dead-function
            // elimination + wasm-opt at OptLevel::RELEASE_DEFAULT (-O3) unless -O overrides.
            // Default (no flag) keeps allocator probes and the full runtime.
            release = true;
        } else if arg == "-g" || arg == "--debug-info" {
            // Source-level debug: MIR DebugLine → C `#line`, clang `-g -O0` for lldb-dap.
            debug_info = true;
        } else if arg == "-h" || arg == "--help" {
            show_help = true;
        } else if arg == "run" {
            run_after_compile = true;
        } else if arg == "build" {
            // Compile only (the default). So `dream build --web file.dream` is not treated as a path.
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
        } else if arg == "--wasm" {
            // Compile to a wasm32 module (.c → clang → .wasm + .wat) instead of a native host.
            native_c = false;
        } else if arg == "-o" || arg == "--output" {
            i += 1;
            let Some(val) = args.get(i) else {
                error!("-o/--output requires a path");
                return ExitCode::FAILURE;
            };
            out_override = Some(val.clone());
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
    // `--web` / `--node` select a JS host and imply wasm32 output.
    if !runtimes.is_empty() {
        native_c = false;
    }
    if !native_c && (debug_adapter || run_after_compile || run_tests) {
        error!("`run`, `test`, and `debug-adapter` execute natively; wasm32 output requires `build` with `--wasm`, `--web`, and/or `--node`");
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
        Target::Wasm32
    })
    .with_release(release)
    .with_debug_info(debug_info)
    .with_runtimes(runtimes)
    .with_compile_targets(compile_targets)
    .with_emit_abi(emit_abi)
    .with_crate_type(crate_type);
    if let Some(level) = optimize {
        compiler = compiler.with_optimize(Some(level));
    }
    let out_path = match out_override {
        Some(path) => path,
        None => match get_path_from_file_path(file_name, release, native_c) {
            Some(path) => path,
            None => {
                error!("Invalid source file path: {}", file_name);
                return ExitCode::FAILURE;
            }
        },
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

            if native_c {
                let cc_opt = OptLevel::from_cli(release, optimize);
                match compile_native_c(Path::new(&out_path), cc_opt, debug_info) {
                    Ok(bin) => {
                        info!(
                            "created file: {}",
                            Path::new(&out_path).with_extension("o").display()
                        );
                        info!("created file: {}", bin.display());
                        if debug_adapter {
                            if let Err(e) =
                                dream::execution::debugger::run_debug_adapter(&bin, &out_path)
                            {
                                error!("Debug adapter failed: {}", e);
                                return ExitCode::FAILURE;
                            }
                            return ExitCode::SUCCESS;
                        }
                        if run_after_compile {
                            info!("Executing native C...");
                            if let Err(e) = run_native_bin(&bin, &out_path, &program_args) {
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
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            use dream::driver::error::CompileError;
            match e {
                CompileError::Syntax | CompileError::Semantic | CompileError::Generator => {}
                CompileError::Io(err) => eprintln!("error: {err}"),
                CompileError::Internal(msg) => eprintln!("error: {msg}"),
            }
            ExitCode::FAILURE
        }
    }
}

/// Prints CLI usage to stderr. Not `error!()`: that prefixes every line with `ERROR`.
fn print_usage(program: &str) {
    eprintln!(
        "\
Usage: {program} [-v|--verbose] [--release] [-g|--debug-info] [-O|--optimize[=LEVEL]] [-o PATH] [--crate-type lib|bin] [--wasm] [--target native|node|web] [--runtime --web|--node] [--filter SUBSTR] [build|run|test|debug-adapter] <file|dir> [-- program-args...]
  -v, --verbose         Print progress information
  --release             Trimmed build; cc default -O3; wasm-opt -O3 (or -Os with --web)
  -g, --debug-info      C `#line` + clang -g -O0 for lldb-dap (`debug-adapter` implies this)
  -O, --optimize[=LVL]  wasm-opt and cc level (LVL: 0-4, s, z, Os, Oz; default: s); overrides --release
  -o, --output PATH     Write guest C here instead of target/debug|release|web
  --wasm                Compile to a wasm32 module (.c + .wasm + .wat) instead of a native host
  --target native|node|web  Compile-time runtime target for availability checks (default: native)
  --runtime             Emit tree-shaken *.(web|node).runtime.js (requires --web and/or --node)
  --web                 Browser-targeted *.web.runtime.js (implies --runtime, wasm output)
  --node                Node-targeted *.node.runtime.js (implies --runtime, wasm output)
  --filter SUBSTR       With `test`: only run @test functions whose names contain SUBSTR
  -h, --help            Show this help message
  build                 Compile only (default; `dream build --web file.dream` is valid)
  run                   Compile native C and execute the .bin
  test                  Discover and run @test functions in a file or directory
  debug-adapter         DAP over stdio via lldb-dap on the native .bin (implies -g)
Example: {program} run src/sample/test_arrays.dream
Example: {program} run tests/cases/process_args_basic.dream -- alpha beta
Example: {program} test tests/
Example: {program} --filter adds test tests/math.dream
Example: {program} --release run src/sample/test_arrays.dream
Example: {program} --wasm sample/interop/js.dream
Example: {program} --web sample/interop/js.dream
Example: {program} --node sample/interop/js.dream
Example: {program} --web --node sample/interop/js.dream
  Wasm artifacts land in target/web/; native C in target/debug/ (or target/release/ with --release)"
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

/// Derives the output `.wat` / `.c` path under `target/`, never beside the source.
///
/// Wasm always uses `target/web/` so hosts do not switch on debug vs `--release`.
/// Native C uses `target/debug/` or `target/release/`.
///
/// Uses the enclosing `dream.toml` directory when one exists; otherwise the source file's
/// directory.
fn get_path_from_file_path(file_path: &str, release: bool, native_c: bool) -> Option<String> {
    let path = Path::new(file_path);
    let file_stem = path.file_stem()?.to_str()?;
    let source_dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let root = find_project_root(path).unwrap_or_else(|| source_dir.to_path_buf());
    let sub = if native_c {
        if release {
            "release"
        } else {
            "debug"
        }
    } else {
        "web"
    };
    let out_dir = root.join("target").join(sub);
    let ext = if native_c { "c" } else { "wat" };
    let result = out_dir.join(format!("{file_stem}.{ext}"));
    Some(result.to_str()?.to_string())
}
