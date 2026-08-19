use clap::{Args, Parser, Subcommand};
use dreamer::commands;
use dreamer::compile_flags::CompileFlags;
use std::path::PathBuf;
use std::process::ExitCode;

/// Same `--release` / `-O` / `--backend` tokens as `dream`.
#[derive(Args, Clone, Debug, Default)]
struct OptFlags {
    /// Trimmed build; wasm-opt / cc default `-O3` (`-Os` with `--web`).
    #[arg(long)]
    release: bool,
    /// wasm-opt and cc level (`0`-`4`, `s`, `z`; bare `-O` is `-Os`). Overrides `--release`.
    #[arg(
        short = 'O',
        long = "optimize",
        num_args = 0..=1,
        default_missing_value = "s",
        value_name = "LVL"
    )]
    optimize: Option<String>,
    /// Compiler backend: `wasm` or `c` (default `c`).
    #[arg(long, value_name = "KIND")]
    backend: Option<String>,
}

impl OptFlags {
    fn into_compile_flags(self) -> anyhow::Result<CompileFlags> {
        CompileFlags::from_cli(self.release, self.optimize, self.backend)
    }
}

#[derive(Parser)]
#[command(
    name = "dreamer",
    version,
    about = "Package manager for the Dream language"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scaffold a new dream.toml + source stub in the current (or given) directory.
    Init {
        /// Project name; defaults to the directory name.
        name: Option<String>,
        /// Directory to create the project in (created if missing). Defaults to the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Comma-separated hosts to declare in dream.toml and scaffold for (`native`, `web`, `node`).
        #[arg(long, value_name = "TARGETS")]
        runtime: Option<String>,
        /// Scaffold a library package (`type = "lib"`, no entry / main).
        #[arg(long)]
        lib: bool,
    },
    /// Add a dependency to dream.toml, then resolve and install it.
    Add {
        name: String,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        git: Option<String>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        rev: Option<String>,
        /// Add under [dev-dependencies] instead of [dependencies].
        #[arg(long)]
        dev: bool,
        /// Workspace member package name (required at a virtual workspace root).
        #[arg(short = 'p', long = "package", value_name = "NAME")]
        package: Option<String>,
    },
    /// Remove a dependency from dream.toml and dream_packages/.
    Remove {
        name: String,
        #[arg(short = 'p', long = "package", value_name = "NAME")]
        package: Option<String>,
    },
    /// Resolve dream.toml into dream.lock and materialize dream_packages/.
    Install,
    /// Re-resolve dependencies to the latest compatible versions.
    Update {
        /// Update only this package (other pins are kept as-is).
        name: Option<String>,
    },
    /// Install dependencies, then compile the project's entry point.
    Build {
        #[command(flatten)]
        opt: OptFlags,
        #[arg(short = 'p', long = "package", value_name = "NAME")]
        package: Option<String>,
    },
    /// Install dependencies, then run the project on the resolved host from dream.toml.
    Run {
        /// Host to run when package.targets lists more than one (`native`, `web`, or `node`).
        #[arg(long, value_name = "HOST")]
        target: Option<String>,
        #[command(flatten)]
        opt: OptFlags,
        /// TCP port for `--target web` (default 8787). Reuses/restarts the previous project server.
        #[arg(long, value_name = "PORT")]
        port: Option<u16>,
        #[arg(short = 'p', long = "package", value_name = "NAME")]
        package: Option<String>,
        /// Extra arguments forwarded to the native program or `node run.mjs`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Install dependencies (incl. dev), then run `@test` suites under `tests/`.
    Test {
        #[command(flatten)]
        opt: OptFlags,
        /// Only run `@test` functions whose names contain this substring.
        #[arg(long, value_name = "SUBSTR")]
        filter: Option<String>,
        #[arg(short = 'p', long = "package", value_name = "NAME")]
        package: Option<String>,
    },
    /// Package the current project and publish it to a registry.
    Publish {
        /// Registry base URL; defaults to [registries] default in dream.toml.
        #[arg(long)]
        registry: Option<String>,
        /// Publish token (GitHub Contents API). Falls back to DREAM_REGISTRY_TOKEN / GITHUB_TOKEN.
        #[arg(long)]
        token: Option<String>,
        #[arg(short = 'p', long = "package", value_name = "NAME")]
        package: Option<String>,
    },
    /// Build a native single-file executable embedding the project's release wasm.
    Pack {
        /// Pack triple (`linux-x64`, `macos-arm64`, …) or `all`. Repeatable; default = host.
        #[arg(long = "target", value_name = "TRIPLE")]
        targets: Vec<String>,
        #[arg(short = 'p', long = "package", value_name = "NAME")]
        package: Option<String>,
    },
    /// Search a registry for packages by name.
    Search { query: String },
    /// Print the resolved dependency tree from dream.lock.
    Tree {
        #[arg(short = 'p', long = "package", value_name = "NAME")]
        package: Option<String>,
    },
    /// Install or manage optional compilers (Zig `cc`, wasi-sdk) under ~/.dream/toolchains/.
    Toolchain {
        #[command(subcommand)]
        cmd: ToolchainCmd,
    },
}

#[derive(Subcommand)]
enum ToolchainCmd {
    /// Download pinned Zig and/or wasi-sdk 33 for this OS/arch.
    Install {
        /// `cc` (Zig) or `wasi-sdk`. Omit to install both.
        component: Option<String>,
    },
    /// Show which toolchain components are installed.
    List,
    /// Remove a component (`cc` or `wasi-sdk`).
    Uninstall { component: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let cwd = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("error: could not determine current directory: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let result = match cli.command {
        Cmd::Init {
            name,
            dir,
            runtime,
            lib,
        } => {
            let dest = dir.unwrap_or(cwd);
            if let Err(e) = std::fs::create_dir_all(&dest) {
                eprintln!("error: could not create {}: {}", dest.display(), e);
                return ExitCode::FAILURE;
            }
            commands::init::run(&dest, name, runtime, lib)
        }
        Cmd::Add {
            name,
            version,
            path,
            git,
            tag,
            branch,
            rev,
            dev,
            package,
        } => commands::add::run(
            &cwd,
            name,
            version,
            path,
            git,
            tag,
            branch,
            rev,
            dev,
            package.as_deref(),
        ),
        Cmd::Remove { name, package } => commands::remove::run(&cwd, &name, package.as_deref()),
        Cmd::Install => commands::install::run(&cwd),
        Cmd::Update { name } => commands::update::run(&cwd, name),
        Cmd::Build { opt, package } => match opt.into_compile_flags() {
            Ok(flags) => commands::build::run_with(&cwd, flags, package.as_deref()),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        },
        Cmd::Run {
            target,
            opt,
            port,
            package,
            args,
        } => match opt.into_compile_flags() {
            Ok(flags) => {
                commands::run::run_with(&cwd, target, flags, port, &args, package.as_deref())
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        },
        Cmd::Test {
            opt,
            filter,
            package,
        } => match opt.into_compile_flags() {
            Ok(flags) => commands::test::run_with(&cwd, flags, filter, package.as_deref()),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        },
        Cmd::Publish {
            registry,
            token,
            package,
        } => commands::publish::run(&cwd, registry, token, package.as_deref()),
        Cmd::Pack { targets, package } => commands::pack::run(&cwd, &targets, package.as_deref()),
        Cmd::Search { query } => commands::search::run(&cwd, &query),
        Cmd::Tree { package } => commands::tree::run(&cwd, package.as_deref()),
        Cmd::Toolchain { cmd } => match cmd {
            ToolchainCmd::Install { component } => commands::toolchain::install(component),
            ToolchainCmd::List => commands::toolchain::list(),
            ToolchainCmd::Uninstall { component } => commands::toolchain::uninstall(component),
        },
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {:#}", e);
            ExitCode::FAILURE
        }
    }
}
