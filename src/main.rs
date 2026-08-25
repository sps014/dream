use clap::{Parser, Subcommand, ValueEnum};
use dream::driver::compiler::{Compiler, Target};
use dream::driver::js_runtime::JsRuntimeTarget;
use dream::driver::ui::{ConsoleReporter, Ui};
use dream::driver::wasm_opt::OptLevel;
use dream::execution::native_c::{compile_native_c, run_native_bin};
use dream_abi::attributes::CompileTargets;
use dream_sema::analyzer::CrateType;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

const EXAMPLES: &str = "\
Examples:
  dream run src/main.dream              compile natively and execute
  dream build --web src/app.dream       wasm32 module + browser JS host in target/web/
  dream --release run src/main.dream    optimized release binary
  dream test tests/                     run @test functions
  dream run app.dream -- alpha beta     pass arguments to the program
  dream fmt src/                        format .dream files in place (--check for CI)

Artifacts land under the enclosing project's target/: native C in target/debug (or
target/release with --release), wasm32 modules in target/web/.";

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

    /// Optimized release build (cc -O3; wasm-opt -O3, -Os for --web)
    #[arg(long, global = true)]
    release: bool,

    /// Emit C `#line` debug info + clang -g -O0 for lldb-dap (`debug-adapter` implies this)
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

    /// Compile a wasm32 module (.c + .wasm + .wat) instead of a native binary
    #[arg(long, global = true)]
    wasm: bool,

    /// Runtime availability target for semantic checks (default: native)
    #[arg(long, value_name = "TARGET", value_enum, ignore_case = true, global = true)]
    target: Option<TargetArg>,

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
    #[arg(long, value_name = "TYPE", value_enum, ignore_case = true, global = true)]
    crate_type: Option<CrateTypeArg>,
}

#[derive(Subcommand)]
enum Command {
    /// Compile only (the default when no subcommand is given)
    Build {
        /// Source .dream file
        file: Option<String>,
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
    },
    /// Format .dream source files in place
    Fmt {
        /// .dream files or directories containing them
        files: Vec<String>,
        /// Fail if any file is unformatted instead of rewriting it (CI mode)
        #[arg(long)]
        check: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Deep per-phase detail goes through tracing (`-v` only); user-facing status/errors go through
    // [`Ui`], so stray library warns do not pollute normal runs.
    let subscriber = FmtSubscriber::builder()
        .with_max_level(if cli.verbose {
            Level::INFO
        } else {
            Level::ERROR
        })
        .without_time()
        .with_target(false)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let ui = Ui::new();

    let run_after_compile = matches!(cli.command, Some(Command::Run { .. }));
    let run_tests = matches!(cli.command, Some(Command::Test { .. }));
    let debug_adapter = matches!(cli.command, Some(Command::DebugAdapter { .. }));
    let debug_info = cli.debug_info || debug_adapter;

    // Resolve the source file and any forwarded program arguments.
    let file_name = match &cli.command {
        Some(Command::Build { file })
        | Some(Command::Run { file, .. })
        | Some(Command::Test { file, .. })
        | Some(Command::DebugAdapter { file }) => file.clone(),
        Some(Command::Fmt { .. }) => None,
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
    let native_c = !cli.wasm && runtimes.is_empty();
    if !native_c && (run_after_compile || run_tests || debug_adapter) {
        ui.error("`run`, `test`, and `debug-adapter` execute natively");
        ui.help("drop --wasm/--web/--node here, or use `dream build --wasm <file>` for a wasm32 module");
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

    if !native_c && (cli.release || optimize.is_some()) && !cfg!(feature = "wasm-opt") {
        ui.error("--release / -O need the compiler built with its `wasm-opt` feature");
        ui.help("rebuild `dream` with default features (cargo build --release)");
        return ExitCode::FAILURE;
    }

    let crate_type = match cli.crate_type.unwrap_or(CrateTypeArg::Bin) {
        CrateTypeArg::Lib => CrateType::Lib,
        CrateTypeArg::Bin => CrateType::Bin,
    };

    let compile_targets = match cli.target {
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
        return match dream::driver::test::run_tests(&path, &opts) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                ui.error(&e);
                ExitCode::FAILURE
            }
        };
    }

    let Some(file_name) = file_name else {
        ui.error("no source file given");
        ui.help("pass a .dream file, e.g. `dream run src/main.dream`");
        return ExitCode::FAILURE;
    };

    ui.step(
        "Compiling",
        &format!("{}{}", file_name, if cli.release { " (--release)" } else { "" }),
    );

    let out_path = match &cli.output {
        Some(path) => path.clone(),
        None => match get_path_from_file_path(&file_name, cli.release, native_c) {
            Some(path) => path,
            None => {
                ui.error(&format!("invalid source file path: {}", file_name));
                return ExitCode::FAILURE;
            }
        },
    };

    if let Some(parent) = Path::new(&out_path).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                ui.error(&format!(
                    "could not create output directory {}: {}",
                    parent.display(),
                    e
                ));
                return ExitCode::FAILURE;
            }
        }
    }

    let reporter = Arc::new(ConsoleReporter::new());
    // `with_release` installs RELEASE_DEFAULT wasm-opt; an explicit `-O` overrides.
    // Always emit `.abi.json`: JS hosts need imports/exports, and native `run` /
    // `debug-adapter` load abi.gpu for `@compute` / shader metadata.
    let mut compiler = Compiler::new(if native_c {
        Target::NativeC
    } else {
        Target::Wasm32
    })
    .with_release(cli.release)
    .with_debug_info(debug_info)
    .with_runtimes(runtimes)
    .with_compile_targets(compile_targets)
    .with_emit_abi(true)
    .with_crate_type(crate_type)
    .with_reporter(reporter.clone());
    if let Some(level) = optimize {
        compiler = compiler.with_optimize(Some(level));
    }

    let start = Instant::now();
    let result = compiler.compile(&file_name, &out_path);
    drop(compiler);

    if let Err(e) = result {
        use dream::driver::error::CompileError;
        match e {
            CompileError::Syntax | CompileError::Semantic | CompileError::Generator => {}
            CompileError::Io(err) => ui.error(&format!("{err}")),
            CompileError::Internal(msg) => {
                report_tool_error(&ui, &msg);
                ui.help("this is an internal compiler error — please report it");
            }
        }
        return ExitCode::FAILURE;
    }

    let elapsed = start.elapsed().as_secs_f64();
    let mut artifacts = reporter.take_artifacts();
    let unoptimized = !cli.release && optimize.is_none() && !debug_adapter;

    if native_c {
        let cc_opt = OptLevel::from_cli(cli.release, optimize);
        let bin = Path::new(&out_path).with_extension("bin");
        ui.step(
            "Compiling C",
            &format!("{} ({})", bin.display(), cc_opt.as_cli_flag()),
        );
        match compile_native_c(Path::new(&out_path), cc_opt, debug_info) {
            Ok(bin) => {
                artifacts.push(bin.clone());
                ui.finish(elapsed, "", &artifacts);
                if unoptimized {
                    ui.debug_build_note(false);
                }
                if debug_adapter {
                    if let Err(e) = dream::execution::debugger::run_debug_adapter(&bin, &out_path) {
                        ui.error(&format!("debug adapter failed: {e}"));
                        return ExitCode::FAILURE;
                    }
                    return ExitCode::SUCCESS;
                }
                if run_after_compile {
                    ui.step("Running", &bin.display().to_string());
                    if let Err(e) = run_native_bin(&bin, &out_path, &program_args) {
                        ui.error(&format!("execution failed: {e}"));
                        return ExitCode::FAILURE;
                    }
                }
            }
            Err(e) => {
                report_tool_error(&ui, &e.to_string());
                if let Some(hint) = dream::driver::c_wasm32::hint_for_failure(&e.to_string()) {
                    ui.help(hint);
                }
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }

    ui.finish(elapsed, "", &artifacts);
    if unoptimized {
        ui.debug_build_note(true);
    }
    ExitCode::SUCCESS
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
