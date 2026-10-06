use clap::{Parser, Subcommand, ValueEnum};
use dream::driver::compiler::{BuildOutcome, Compiler};
use dream::driver::js_runtime::JsRuntimeTarget;
use dream::driver::ui::{ConsoleReporter, Ui};
use dream::driver::wasm_opt::OptLevel;
use dream::execution::llvm::build::emit_llvm_artifacts;
use dream::execution::llvm::compile_llvm;
use dream::execution::native::{run_native_bin, GuestAborted, Pgo};
use dream_abi::attributes::CompileTargets;
use dream_sema::analyzer::CrateType;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

mod generate_cmd;

const EXAMPLES: &str = "\
Examples:
  dreamer run                           project: uses package.entry from dream.toml
  dream run                             same, if dream.toml is in this directory (or above)
  dream run src/main.dream              compile natively and execute a file
  dream build --web src/app.dream       wasm32 module + browser JS host in target/web/
  dream --release run src/main.dream    optimized release binary
  dream test tests/                     run @test functions
  dream run app.dream -- alpha beta     pass arguments to the program
  dream fmt src/                        format .dream files in place (--check for CI)

Artifacts land under the enclosing project's target/: native builds (.ll + binary) in
target/debug (or target/release with --release), wasm32 modules in target/web/. Prefer `dreamer run`
for packages (deps, web/node hosts). Use `dream run <file>` for a one-off source file.";

#[derive(Copy, Clone, ValueEnum)]
enum TargetArg {
    Native,
    Node,
    Web,
}

#[derive(Copy, Clone, ValueEnum)]
enum CrateTypeArg {
    Lib,
    Bin,
}

#[derive(Parser)]
#[command(
    name = "dream",
    version,
    about = "The Dream programming language compiler",
    after_help = EXAMPLES,
    after_long_help = EXAMPLES
)]
struct Cli {
    /// Source .dream file (or tests/ directory with `test`)
    file: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,

    /// Print per-phase progress detail
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Optimized release build (LLVM -O3; wasm-opt -O3, -Os for --web)
    #[arg(long, global = true)]
    release: bool,

    /// Emit DWARF and build at -O0 for lldb-dap (`debug-adapter` implies this)
    #[arg(short = 'g', long = "debug-info", global = true)]
    debug_info: bool,

    /// Optimization level (0-4, s, z); bare -O means Os; overrides --release
    #[arg(
        short = 'O',
        long = "optimize",
        value_name = "LEVEL",
        num_args = 0..=1,
        default_missing_value = "s",
        global = true
    )]
    optimize: Option<String>,

    /// Write output here instead of the project's target/ directory
    #[arg(short = 'o', long = "output", value_name = "PATH", global = true)]
    output: Option<String>,

    /// Compile a wasm32 module (.opt.ll + .wasm + .wat) instead of a native binary
    #[arg(long, global = true)]
    wasm: bool,

    /// Stop after the native module's LLVM IR: writes the optimized .opt.ll and .s
    #[arg(long = "emit-llvm", global = true)]
    emit_llvm: bool,

    /// Bundle required host libraries beside the native binary with package-relative lookup
    #[arg(long, global = true, conflicts_with_all = ["wasm", "emit_llvm"])]
    relocatable: bool,

    /// App icon PNG compiled into a native binary (dreamer passes `[package].icon`)
    #[arg(long, value_name = "PNG", global = true, hide = true)]
    icon: Option<PathBuf>,

    /// LLVM target triple for compilation and linking
    #[arg(long, value_name = "TRIPLE", global = true, conflicts_with_all = ["wasm", "web", "node"])]
    target: Option<String>,

    /// Stop at an unlinked target object (SDK and capability libraries are not required)
    #[arg(long, global = true, conflicts_with_all = ["wasm", "web", "node", "emit_llvm", "relocatable", "profile", "use_profile", "icon"])]
    object: bool,

    /// Runtime availability target for semantic checks (default: native)
    #[arg(
        long = "runtime-target",
        value_name = "TARGET",
        value_enum,
        ignore_case = true,
        global = true
    )]
    runtime_target: Option<TargetArg>,

    /// Minimum macOS version encoded in the LLVM target triple
    #[arg(long, value_name = "VERSION", global = true)]
    min_os: Option<dream_abi::target::OsVersion>,

    /// Emit tree-shaken *.(web|node).runtime.js hosts (requires --web and/or --node)
    #[arg(long, global = true)]
    runtime: bool,

    /// Browser-targeted *.web.runtime.js (implies --runtime and wasm32 output)
    #[arg(long, global = true)]
    web: bool,

    /// Node-targeted *.node.runtime.js (implies --runtime and wasm32 output)
    #[arg(long, global = true)]
    node: bool,

    /// Library vs binary crate (libs reject a primary-file `main`)
    #[arg(
        long,
        value_name = "TYPE",
        value_enum,
        ignore_case = true,
        global = true
    )]
    crate_type: Option<CrateTypeArg>,

    /// Compiler debugging: dump MIR to <output>.mir/<NN>-<pass>.mir
    /// (`after:<pass>`, `after:<pass>,each`, or `all`)
    #[arg(long = "emit-mir", value_name = "WHEN", global = true)]
    emit_mir: Option<String>,

    /// Limit --emit-mir to these functions (comma-separated exact names)
    #[arg(
        long = "emit-mir-fn",
        value_name = "NAMES",
        global = true,
        requires = "emit_mir"
    )]
    emit_mir_fn: Option<String>,

    /// Native PGO step 1: build an instrumented binary; its runs record profiles into
    /// <output>.pgo/ next to it
    #[arg(long, global = true, conflicts_with = "use_profile")]
    profile: bool,

    /// Native PGO step 2: optimize with a profile (default: merge the --profile runs; or
    /// --use-profile=<.profdata | .profraw | directory of .profraw files>)
    #[arg(
        long = "use-profile",
        value_name = "PROFILE",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "",
        global = true
    )]
    use_profile: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    #[command(hide = true)]
    ToolchainDoctor {
        #[arg(long)]
        json: bool,
    },
    /// Compile only (the default when no subcommand is given)
    Build {
        /// Source .dream file
        file: Option<String>,
        /// Serve a DAP session on this generator (as `debug-adapter --generator`), then build
        #[arg(long, value_name = "GEN")]
        debug_generator: Option<String>,
    },
    /// Compile natively and execute immediately
    Run {
        /// Source .dream file
        file: Option<String>,
        /// Arguments forwarded to the compiled program (everything after `--`)
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Discover and run @test functions in a .dream file or directory
    Test {
        /// .dream file or directory of them (defaults to ./tests)
        file: Option<String>,
        /// Only run @test functions whose names contain this substring
        #[arg(long, value_name = "SUBSTR")]
        filter: Option<String>,
    },
    /// Serve DAP over stdio via lldb-dap on the native .bin (implies -g)
    DebugAdapter {
        /// Source .dream file
        file: Option<String>,
        /// Debug this compile-time generator on the program's snapshot instead of the program
        #[arg(long, value_name = "GEN")]
        generator: Option<String>,
        /// With --generator: a captured snapshot (file or `--capture` directory) to run on
        #[arg(long, value_name = "PATH", requires = "generator")]
        snapshot: Option<PathBuf>,
    },
    /// Inspect compile-time generators: list them, explain cache keys, capture and replay runs
    Generate {
        /// Source .dream file
        file: Option<String>,
        /// Registered generators, their triggers, and whether each runs (the default)
        #[arg(long)]
        list: bool,
        /// Each cache-key component of a generator and what changed since the last build
        #[arg(long, value_name = "GEN")]
        explain: Option<String>,
        /// Run a generator once and save its snapshot and result (into -o, or
        /// target/generators/<GEN>)
        #[arg(long, value_name = "GEN")]
        capture: Option<String>,
        /// Rerun a --capture directory without compiling, comparing against its result
        #[arg(long, value_name = "DIR")]
        replay: Option<PathBuf>,
        /// Rerun @incremental generators that would replay a cached result and diff the results
        #[arg(long)]
        verify_incremental: bool,
        /// Build every std generator executable into the cache (no source file needed)
        #[arg(long, conflicts_with = "file")]
        prewarm: bool,
    },
    /// Format .dream source files in place
    Fmt {
        /// .dream files or directories containing them
        files: Vec<String>,
        /// Fail if any file is unformatted instead of rewriting it (CI mode)
        #[arg(long)]
        check: bool,
    },
    /// Build the prebuilt runtime tree a release ships as lib/dream/rt (needs a full LLVM)
    #[command(hide = true)]
    PackRuntime {
        /// Output directory
        out: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = Arc::new(dream::driver::toolchain::ToolchainConfig::default());

    // Deep per-phase detail goes through tracing (`-v` only); user-facing status/errors go through
    // [`Ui`], so stray library warns do not pollute normal runs.
    let subscriber = FmtSubscriber::builder()
        .with_max_level(if cli.verbose {
            Level::INFO
        } else {
            Level::ERROR
        })
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .with_timer(())
        .with_ansi(dream::driver::ui::color_enabled())
        .with_target(false)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let ui = Ui::new();

    if let Some(Command::ToolchainDoctor { json }) = &cli.command {
        return match dream::execution::llvm::doctor::run(
            config.clone(),
            cli.target.as_deref(),
            *json,
        ) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(error) => {
                ui.error(&error);
                ExitCode::FAILURE
            }
        };
    }
    if let Some(Command::PackRuntime { out }) = &cli.command {
        return match dream::execution::llvm::pack_runtime(&config, out) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                ui.error(&e);
                ExitCode::FAILURE
            }
        };
    }

    let run_after_compile = matches!(cli.command, Some(Command::Run { .. }));
    let run_tests = matches!(cli.command, Some(Command::Test { .. }));
    let debug_adapter = matches!(
        cli.command,
        Some(Command::DebugAdapter {
            generator: None,
            ..
        })
    );
    let debug_info = cli.debug_info || debug_adapter;

    // Resolve the source file and any forwarded program arguments.
    let file_name = match &cli.command {
        Some(Command::Build { file, .. })
        | Some(Command::Run { file, .. })
        | Some(Command::Test { file, .. })
        | Some(Command::DebugAdapter { file, .. })
        | Some(Command::Generate {
            file,
            prewarm: false,
            ..
        }) => file.clone(),
        Some(Command::Generate { prewarm: true, .. }) => {
            match dream::driver::generate::commands::prewarm_entry(&config) {
                Ok(entry) => Some(entry.to_string_lossy().into_owned()),
                Err(e) => {
                    ui.error(&format!("cannot write the prewarm program: {e}"));
                    return ExitCode::FAILURE;
                }
            }
        }
        Some(Command::Fmt { .. })
        | Some(Command::PackRuntime { .. })
        | Some(Command::ToolchainDoctor { .. }) => None,
        None => cli.file.clone(),
    };
    let program_args = match &cli.command {
        Some(Command::Run { args, .. }) => args.clone(),
        _ => Vec::new(),
    };

    // `--web` / `--node` select JS hosts and imply wasm32 output.
    let mut runtimes = Vec::new();
    if cli.web {
        runtimes.push(JsRuntimeTarget::Web);
    }
    if cli.node {
        runtimes.push(JsRuntimeTarget::Node);
    }
    if cli.runtime && runtimes.is_empty() {
        ui.error("--runtime needs at least one host: pass --web and/or --node");
        return ExitCode::FAILURE;
    }
    let native = !cli.wasm && runtimes.is_empty();
    if cli.relocatable
        && (!native || run_tests || matches!(cli.crate_type, Some(CrateTypeArg::Lib)))
    {
        ui.error("--relocatable applies to native executable builds only");
        return ExitCode::FAILURE;
    }
    if cli.emit_llvm && !native {
        ui.error("--emit-llvm writes the native whole-program module");
        ui.help("drop --wasm/--web/--node; wasm32 builds keep the generated .ll anyway");
        return ExitCode::FAILURE;
    }
    if cli.emit_llvm && (run_after_compile || run_tests) {
        ui.error("--emit-llvm stops after writing the .ll; drop it to run");
        return ExitCode::FAILURE;
    }
    if !native && (run_after_compile || run_tests || debug_adapter) {
        ui.error("`run`, `test`, and `debug-adapter` execute natively");
        ui.help(
            "drop --wasm/--web/--node here, or use `dream build --wasm <file>` for a wasm32 module",
        );
        return ExitCode::FAILURE;
    }
    if (!native || run_tests || debug_adapter) && (cli.profile || cli.use_profile.is_some()) {
        ui.error("--profile / --use-profile apply to native `build` / `run` only");
        return ExitCode::FAILURE;
    }

    let optimize = match &cli.optimize {
        Some(level_str) => match level_str.parse::<OptLevel>() {
            Ok(level) => Some(level),
            Err(e) => {
                ui.error(&e);
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    let emit_mir = match &cli.emit_mir {
        Some(spec) => match dream_mir::passes::MirDumpSpec::parse(spec, cli.emit_mir_fn.as_deref())
        {
            Ok(spec) => Some(spec),
            Err(e) => {
                ui.error(&e);
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    let manifest_start = file_name
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let manifest = match dream::driver::project_manifest::find_project_root_from(&manifest_start) {
        Some(root) => match dream::driver::project_manifest::ProjectManifest::load(&root) {
            Ok(manifest) => Some(manifest),
            Err(error) => {
                ui.error(&error);
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let library = manifest.as_ref().and_then(|m| m.library.as_ref());
    let output_kind = if let Some(library) = library {
        if !native
            || run_after_compile
            || run_tests
            || debug_adapter
            || cli.emit_llvm
            || cli.relocatable
            || cli.profile
            || cli.use_profile.is_some()
            || cli.icon.is_some()
            || matches!(cli.crate_type, Some(CrateTypeArg::Bin))
        {
            ui.error(
                "[lib].output-type requires a native library build without executable-only options",
            );
            return ExitCode::FAILURE;
        }
        match library.output_type {
            dream_abi::library::LibraryKind::Staticlib => {
                dream::driver::output::OutputKind::Staticlib
            }
            dream_abi::library::LibraryKind::Cdylib => dream::driver::output::OutputKind::Dylib,
        }
    } else if native {
        if matches!(cli.crate_type, Some(CrateTypeArg::Lib))
            && cli.target.is_none()
            && !cli.emit_llvm
        {
            ui.error("native libraries require [lib].output-type = \"staticlib\" or \"cdylib\" in dream.toml");
            return ExitCode::FAILURE;
        }
        dream::driver::output::OutputKind::Executable
    } else {
        dream::driver::output::OutputKind::Wasm
    };
    let crate_type = match cli.crate_type.unwrap_or(if output_kind.is_library() {
        CrateTypeArg::Lib
    } else {
        CrateTypeArg::Bin
    }) {
        CrateTypeArg::Lib => CrateType::Lib,
        CrateTypeArg::Bin => CrateType::Bin,
    };

    let compile_targets = match cli.runtime_target {
        Some(TargetArg::Native) => CompileTargets::native_only(),
        Some(TargetArg::Node) => CompileTargets {
            native: false,
            node: true,
            web: false,
        },
        Some(TargetArg::Web) => CompileTargets {
            native: false,
            node: false,
            web: true,
        },
        None => CompileTargets {
            native: runtimes.is_empty(),
            node: cli.node,
            web: cli.web,
        },
    };

    if let Some(Command::Fmt { files, check }) = &cli.command {
        return run_fmt(&ui, files, *check);
    }

    let target =
        match dream::driver::target::resolve_triple(!native, cli.target.as_deref(), cli.min_os) {
            Ok(target) => target,
            Err(error) => {
                ui.error(&error);
                return ExitCode::FAILURE;
            }
        };
    if (run_after_compile || run_tests || debug_adapter)
        && (cli.object || (native && !target.spec().can_link_on_host()))
    {
        ui.error("run, test and debug-adapter require a linked host executable");
        return ExitCode::FAILURE;
    }
    if cli.object && (!native || cli.emit_llvm) {
        ui.error("--object requires native output and cannot be combined with --emit-llvm");
        return ExitCode::FAILURE;
    }

    if run_tests {
        let path = match file_name {
            Some(name) => PathBuf::from(name),
            None => {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let tests = cwd.join("tests");
                if tests.is_dir() {
                    tests
                } else {
                    ui.error("no test files given and no tests/ directory found");
                    ui.help("pass a .dream file or a directory containing them");
                    return ExitCode::FAILURE;
                }
            }
        };
        let opts = dream::driver::test::TestOptions {
            release: cli.release,
            optimize,
            filter: match &cli.command {
                Some(Command::Test { filter, .. }) => filter.clone(),
                _ => None,
            },
            verbose: cli.verbose,
        };
        return match dream::driver::test::run_tests(&config, &path, &opts) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                ui.error(&e);
                ExitCode::FAILURE
            }
        };
    }

    match &cli.command {
        Some(Command::Generate {
            replay: Some(dir), ..
        }) => return generate_cmd::replay(&ui, &config, dir),
        Some(Command::DebugAdapter {
            generator: Some(gen_name),
            snapshot: Some(dir),
            ..
        }) if dream::driver::generate::commands::is_capture_dir(dir) => {
            return generate_cmd::debug_capture(&ui, &config, dir, gen_name);
        }
        _ => {}
    }

    let Some(file_name) = file_name.or_else(|| {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        dream::driver::generate::default_compile_entry(&cwd)
            .and_then(|p| p.to_str().map(|s| s.to_string()))
    }) else {
        ui.error("no source file given");
        ui.help("pass a .dream file, or run from a project with package.entry in dream.toml (`dreamer run`)");
        return ExitCode::FAILURE;
    };

    let inspector = || {
        Compiler::new_with_toolchain_config(target.clone(), config.clone())
            .with_compile_targets(compile_targets)
            .with_crate_type(crate_type)
    };
    match &cli.command {
        Some(Command::Generate {
            explain,
            capture,
            verify_incremental,
            prewarm,
            ..
        }) => {
            let action = if *prewarm {
                generate_cmd::GenerateAction::Prewarm
            } else if let Some(gen_name) = explain {
                generate_cmd::GenerateAction::Explain(gen_name.clone())
            } else if let Some(gen_name) = capture {
                generate_cmd::GenerateAction::Capture(
                    gen_name.clone(),
                    cli.output.clone().map(PathBuf::from),
                )
            } else if *verify_incremental {
                generate_cmd::GenerateAction::VerifyIncremental
            } else {
                generate_cmd::GenerateAction::List
            };
            return generate_cmd::run_generate(&ui, &config, &inspector(), &file_name, action);
        }
        Some(Command::DebugAdapter {
            generator: Some(gen_name),
            snapshot,
            ..
        }) => {
            return generate_cmd::debug_generator(
                &ui,
                &config,
                &inspector(),
                &file_name,
                gen_name,
                snapshot.as_deref(),
            );
        }
        Some(Command::Build {
            debug_generator: Some(gen_name),
            ..
        }) => {
            let code =
                generate_cmd::debug_generator(&ui, &config, &inspector(), &file_name, gen_name, None);
            if code != ExitCode::SUCCESS {
                return code;
            }
        }
        _ => {}
    }

    ui.step(
        "Compiling",
        &format!(
            "{}{}",
            file_name,
            if cli.release { " (--release)" } else { "" }
        ),
    );

    let out_path = match &cli.output {
        Some(path) if output_kind.is_library() => Path::new(path)
            .with_extension("ll")
            .to_string_lossy()
            .into_owned(),
        Some(path) => path.clone(),
        None => match get_path_from_file_path(&file_name, cli.release, native) {
            Some(path) => path,
            None => {
                ui.error(&format!("invalid source file path: {}", file_name));
                return ExitCode::FAILURE;
            }
        },
    };

    if let Some(parent) = Path::new(&out_path).parent()
        && !parent.as_os_str().is_empty()
            && let Err(e) = std::fs::create_dir_all(parent) {
                ui.error(&format!(
                    "could not create output directory {}: {}",
                    parent.display(),
                    e
                ));
                return ExitCode::FAILURE;
            }

    let reporter = Arc::new(ConsoleReporter::new());
    // `with_release` installs RELEASE_DEFAULT wasm-opt; an explicit `-O` overrides.
    let cc_opt = OptLevel::from_cli(cli.release, optimize);
    // Profiles change between runs without touching any compiler input, so PGO builds never reuse.
    let build_cache = (!cli.profile && cli.use_profile.is_none()).then(|| {
        let icon = cli
            .icon
            .clone()
            .map(|icon| dream::driver::rt_stamp::fingerprint(vec![icon]))
            .unwrap_or_default();
        format!(
            "{}|{debug_info}|{:?}|{}|{:?}|{native}|{icon}",
            cc_opt.as_cli_flag(),
            cli.target,
            cli.emit_llvm,
            (cli.relocatable, output_kind, cli.object),
        )
    });
    let mut compiler = Compiler::new_with_toolchain_config(target.clone(), config.clone())
        .with_build_cache(build_cache)
        .with_release(cli.release)
        .with_debug_info(debug_info)
        .with_runtimes(runtimes)
        .with_compile_targets(compile_targets)
        .with_crate_type(crate_type)
        .with_output_kind(output_kind)
        .with_emit_mir(emit_mir)
        .with_opt_ir(true)
        .with_reporter(reporter.clone());
    if let Some(level) = optimize {
        compiler = compiler.with_optimize(Some(level));
    }

    let _memory = dream::driver::metrics::PeakMemoryReport::new(cli.verbose);
    let start = Instant::now();
    let result = compiler.compile(&file_name, &out_path);
    drop(compiler);

    let outcome = match result {
        Ok(outcome) => outcome,
        Err(e) => {
            report_compile_error(&ui, e);
            return ExitCode::FAILURE;
        }
    };
    let unoptimized = !cli.release && optimize.is_none() && !debug_adapter;
    let launch = Launch {
        config: &config,
        out_path: &out_path,
        program_args: &program_args,
        debug_adapter,
        run_after_compile,
    };
    let stamp = match outcome {
        BuildOutcome::Built(stamp) => stamp,
        BuildOutcome::Cached(artifacts) => {
            ui.finish(start.elapsed().as_secs_f64(), "", &artifacts);
            let linked = native && !cli.object && !cli.emit_llvm && !output_kind.is_library();
            if unoptimized {
                ui.debug_build_note(!linked);
            }
            drop(_memory);
            if linked {
                return launch.run(
                    &ui,
                    &output_kind.artifact_path(Path::new(&out_path), target.spec()),
                );
            }
            return ExitCode::SUCCESS;
        }
    };
    let record = |artifacts: &[PathBuf]| {
        if let Some(stamp) = &stamp {
            stamp.store(artifacts);
        }
    };

    let elapsed = start.elapsed().as_secs_f64();
    let mut artifacts = reporter.take_artifacts();
    let raw_ll = if native {
        PathBuf::from(&out_path)
    } else {
        Path::new(&out_path).with_extension("ll")
    };
    let raw_ll = raw_ll.as_path();
    let drop_raw_ll = || {
        let _ = std::fs::remove_file(raw_ll);
        let _ = std::fs::remove_file(dream::driver::compiler::c_shim_path(raw_ll));
    };

    if cli.object {
        match dream::execution::llvm::cross::emit_object(&config, target.spec(), raw_ll, cc_opt) {
            Ok(object) => artifacts.push(object),
            Err(error) => {
                ui.error(&error);
                return ExitCode::FAILURE;
            }
        }
        record(&artifacts);
        ui.finish(start.elapsed().as_secs_f64(), "", &artifacts);
        return ExitCode::SUCCESS;
    }

    if cli.emit_llvm {
        match emit_llvm_artifacts(
            &config,
            target.spec(),
            raw_ll,
            cc_opt,
            debug_info,
            cli.icon.as_deref(),
        ) {
            Ok(paths) => {
                drop_raw_ll();
                artifacts.extend(paths);
            }
            Err(e) => {
                ui.error(&e.to_string());
                return ExitCode::FAILURE;
            }
        }
        record(&artifacts);
        ui.finish(start.elapsed().as_secs_f64(), "", &artifacts);
        return ExitCode::SUCCESS;
    }
    if native {
        let bin = output_kind.artifact_path(Path::new(&out_path), target.spec());
        ui.step(
            "Linking",
            &format!("{} ({})", bin.display(), cc_opt.as_cli_flag()),
        );
        let pgo = match &cli.use_profile {
            _ if cli.profile => Pgo::Generate,
            Some(p) if p.is_empty() => Pgo::Use(None),
            Some(p) => Pgo::Use(Some(PathBuf::from(p))),
            None => Pgo::Off,
        };
        let opt_ll = raw_ll.with_extension("opt.ll");
        return match compile_llvm(
            &config,
            raw_ll,
            dream::execution::llvm::NativeBuildOptions {
                target: target.spec().clone(),
                opt_ll: Some(&opt_ll),
                opt: cc_opt,
                debug: debug_info,
                pgo: &pgo,
                icon: cli.icon.as_deref(),
                relocatable: cli.relocatable,
                output_kind,
            },
        ) {
            Ok(bin) => {
                drop_raw_ll();
                artifacts.push(opt_ll);
                artifacts.push(bin.clone());
                if output_kind == dream::driver::output::OutputKind::Staticlib {
                    artifacts.push(raw_ll.with_extension("link.json"));
                } else if target.spec().is_windows()
                    && output_kind == dream::driver::output::OutputKind::Dylib
                {
                    artifacts.push(bin.with_extension("lib"));
                }
                record(&artifacts);
                ui.finish(elapsed, "", &artifacts);
                if unoptimized {
                    ui.debug_build_note(false);
                }
                drop(_memory);
                if output_kind.is_library() {
                    ExitCode::SUCCESS
                } else {
                    launch.run(&ui, &bin)
                }
            }
            Err(e) => {
                report_tool_error(&ui, &e.to_string());
                if let Some(hint) = dream::driver::wasi::hint_for_failure(&e.to_string()) {
                    ui.help(hint);
                }
                ExitCode::FAILURE
            }
        };
    }

    drop_raw_ll();
    record(&artifacts);
    ui.finish(elapsed, "", &artifacts);
    if unoptimized {
        ui.debug_build_note(true);
    }
    ExitCode::SUCCESS
}

/// What to do with a linked native binary once the build (fresh or cached) is in place.
struct Launch<'a> {
    config: &'a Arc<dream::driver::toolchain::ToolchainConfig>,
    out_path: &'a str,
    program_args: &'a [String],
    debug_adapter: bool,
    run_after_compile: bool,
}

impl Launch<'_> {
    fn run(&self, ui: &Ui, bin: &Path) -> ExitCode {
        if self.debug_adapter {
            if let Err(e) =
                dream::execution::debugger::run_debug_adapter(
                    self.config,
                    bin,
                    self.out_path,
                    &Path::new(self.out_path).with_extension("opt.ll"),
                    &[],
                )
            {
                ui.error(&format!("debug adapter failed: {e}"));
                return ExitCode::FAILURE;
            }
            return ExitCode::SUCCESS;
        }
        if !self.run_after_compile {
            return ExitCode::SUCCESS;
        }
        ui.step("Running", &bin.display().to_string());
        // The guest's exit status is the program's own (`main(): int`, or a failing `Result`),
        // so forward it instead of reporting a tool failure.
        match run_native_bin(self.config, bin, self.out_path, self.program_args) {
            Ok(0) => ExitCode::SUCCESS,
            Ok(code) => ExitCode::from(code.clamp(1, 255) as u8),
            // `dream_panic` / `abort()` already printed the crash on stderr.
            Err(e) if e.downcast_ref::<GuestAborted>().is_some() => ExitCode::FAILURE,
            Err(e) => {
                ui.error(&format!("execution failed: {e}"));
                ExitCode::FAILURE
            }
        }
    }
}

fn report_compile_error(ui: &Ui, e: dream::driver::error::CompileError) {
    use dream::driver::error::CompileError;
    match e {
        CompileError::Syntax(_) | CompileError::Semantic(_) | CompileError::Generator(_) => {}
        CompileError::Io(err) => ui.error(&format!("{err}")),
        CompileError::Manifest(msg) => ui.error(&msg),
        CompileError::Toolchain(msg) => ui.error(&msg),
        CompileError::Internal(msg) => {
            report_tool_error(ui, &msg);
            ui.help("this is an internal compiler error — please report it");
        }
    }
}

/// Reports a compiler/toolchain failure: the first line becomes the bold `error:` header and any
/// captured tool output (clang diagnostics, …) follows dimmed and indented.
fn report_tool_error(ui: &Ui, msg: &str) {
    match msg.split_once('\n') {
        Some((head, rest)) => ui.error_with_detail(head, rest),
        None => ui.error(msg),
    }
}

/// `dream fmt`: formats the given files/directories in place (or checks them under
/// `--check`). Files the formatter cannot safely rewrite (lex errors) are reported and
/// skipped; a directory expands recursively to its `.dream` files.
fn run_fmt(ui: &Ui, paths: &[String], check: bool) -> ExitCode {
    let mut files = Vec::new();
    for raw in paths {
        let path = Path::new(raw);
        if !path.exists() {
            ui.error(&format!("no such file or directory: {raw}"));
            return ExitCode::FAILURE;
        }
        collect_dream_files(path, &mut files);
    }
    if files.is_empty() {
        ui.error("no .dream files found");
        ui.help("pass a .dream file or a directory containing them");
        return ExitCode::FAILURE;
    }
    // Deterministic order so output is stable regardless of directory iteration.
    files.sort();
    files.dedup();

    let mut unformatted: Vec<PathBuf> = Vec::new();
    for file in &files {
        let Ok(source) = std::fs::read_to_string(file) else {
            ui.warning(&format!("could not read {}", file.display()));
            continue;
        };
        let Some(formatted) = dream_format::try_format(&source) else {
            ui.warning(&format!(
                "skipped {} (does not lex cleanly — fix syntax errors first)",
                file.display()
            ));
            continue;
        };
        if formatted == source {
            continue;
        }
        if check {
            unformatted.push(file.clone());
        } else if std::fs::write(file, formatted).is_err() {
            ui.error(&format!("could not write {}", file.display()));
            return ExitCode::FAILURE;
        } else {
            ui.success(&format!("formatted {}", file.display()));
        }
    }

    if check {
        for file in &unformatted {
            ui.note(&format!("{} needs formatting", file.display()));
        }
        if unformatted.is_empty() {
            ui.success("all files are formatted");
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    } else {
        ExitCode::SUCCESS
    }
}

/// Appends `path` and, for directories, every `.dream` file beneath it (recursively).
fn collect_dream_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            collect_dream_files(&entry.path(), out);
        }
    } else if path.extension().map(|e| e == "dream").unwrap_or(false) {
        out.push(path.to_path_buf());
    }
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

/// Derives the output `.wat` / `.ll` path under `target/`, never beside the source.
///
/// Wasm always uses `target/web/` so hosts do not switch on debug vs `--release`.
/// Native builds use `target/debug/` or `target/release/`.
///
/// Uses the enclosing `dream.toml` directory when one exists; otherwise the source file's
/// directory.
fn get_path_from_file_path(file_path: &str, release: bool, native: bool) -> Option<String> {
    let path = Path::new(file_path);
    let file_stem = path.file_stem()?.to_str()?;
    let source_dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let root = find_project_root(path).unwrap_or_else(|| source_dir.to_path_buf());
    let sub = if native {
        if release {
            "release"
        } else {
            "debug"
        }
    } else {
        "web"
    };
    let out_dir = root.join("target").join(sub);
    let ext = if native { "ll" } else { "wat" };
    let result = out_dir.join(format!("{file_stem}.{ext}"));
    Some(result.to_str()?.to_string())
}
