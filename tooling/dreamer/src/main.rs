mod init_cli;

use clap::{Args, Parser, Subcommand};
use dreamer::commands;
use dreamer::compile_flags::CompileFlags;
use std::fmt::Display;
use std::process::ExitCode;
use yansi::Paint;

/// Same `--release` / `-O` / `--wasm` tokens as `dream`.
#[derive(Args, Clone, Debug, Default)]
struct OptFlags {
    /// Trimmed build; wasm-opt / native default `-O3` (`-Os` with `--web`).
    #[arg(long)]
    release: bool,
    /// Release wasm-opt and native level (`0`-`4`, `s`, `z`; bare `-O` is `-Os`). Requires `--release`.
    #[arg(
        short = 'O',
        long = "optimize",
        num_args = 0..=1,
        default_missing_value = "s",
        value_name = "LVL"
    )]
    optimize: Option<String>,
    /// Compile to a wasm32 module instead of a native host.
    #[arg(long)]
    wasm: bool,
    /// Native PGO step 1: build a clang-instrumented binary that records profiles.
    #[arg(long, conflicts_with = "use_profile")]
    profile: bool,
    /// Native PGO step 2: optimize with the recorded profiles (or `--use-profile=<path>`).
    #[arg(
        long = "use-profile",
        value_name = "PROFILE",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = ""
    )]
    use_profile: Option<String>,
}

impl OptFlags {
    fn into_compile_flags(self) -> anyhow::Result<CompileFlags> {
        let flags = CompileFlags::from_cli(self.release, self.optimize, self.wasm)?;
        flags.with_pgo(self.profile, self.use_profile)
    }
}

#[derive(Parser)]
#[command(
    name = "dreamer",
    version,
    about = "Package manager for the Dream language",
    after_help = "Examples:\n  dreamer init\n  dreamer init mobile phone\n  dreamer init app counter\n  dreamer init web my-app\n  dreamer add system.testing\n  dreamer run --release\n  dreamer test --filter math\n  dreamer toolchain install binaryen",
    after_long_help = "Examples:\n  dreamer init                          scaffold a console project here\n  dreamer init mobile phone             scaffold an Android/iOS canvas project\n  dreamer init app counter              scaffold a desktop + mobile canvas project\n  dreamer init desktop counter          scaffold a desktop canvas app\n  dreamer init web my-app               scaffold a browser project\n  dreamer add system.testing            add a dependency from the registry\n  dreamer add ./local/pkg --path        add a path dependency\n  dreamer run --release                 install deps, build, and run\n  dreamer test --filter math            run @test suites under tests/\n  dreamer pack                          relocatable native executable (default --release / -O3)\n  dreamer toolchain install binaryen    WASM optimizer for release WASM builds"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List Android devices and available iOS simulators without booting them.
    Devices {
        /// Restrict discovery to android or ios.
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Create a console, desktop, mobile, app, web, node, lib, test, or workspace project.
    Init(init_cli::InitArgs),
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
        /// Build for native, web, node, android (debug APK), or ios (simulator app).
        #[arg(long)]
        target: Option<String>,
        #[command(flatten)]
        opt: OptFlags,
        #[arg(short = 'p', long = "package", value_name = "NAME")]
        package: Option<String>,
    },
    /// Install dependencies, then run the project on the resolved host from dream.toml.
    Run {
        /// Select native, web, node, android, or ios.
        #[arg(long, value_name = "HOST")]
        target: Option<String>,
        /// Android serial or iOS simulator UDID from `dreamer devices`; optional when exactly one
        /// device is running.
        #[arg(long, value_name = "ID")]
        device: Option<String>,
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
    /// Install dependencies (incl. dev), then build and run test projects: every test project in
    /// the workspace, the selected `-p` one, or this package's (`type = "test"` or `tests/`).
    Test {
        #[command(flatten)]
        opt: OptFlags,
        /// Only run tests whose `<file>::<name>` contains this substring.
        #[arg(long, value_name = "SUBSTR")]
        filter: Option<String>,
        #[arg(short = 'p', long = "package", value_name = "NAME")]
        package: Option<String>,
        /// Run on the native host instead of each project's [test].targets (with --node: both).
        #[arg(long)]
        native: bool,
        /// Run under Node instead of each project's [test].targets (with --native: both).
        #[arg(long)]
        node: bool,
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
    /// Build a host app bundle, or package target-built iOS/Android libraries.
    Pack {
        #[command(flatten)]
        opt: OptFlags,
        /// Pack target: a host triple, ios, or android. Default = host.
        #[arg(long = "target", value_name = "TRIPLE")]
        targets: Vec<String>,
        #[arg(short = 'p', long = "package", value_name = "NAME")]
        package: Option<String>,
        /// Target-built mobile library, TRIPLE=PATH, with adjacent .h and .abi.json
        #[arg(long = "slice", value_name = "TRIPLE=LIBRARY")]
        slices: Vec<String>,
        #[arg(long, value_name = "JAVA_PACKAGE")]
        android_package: Option<String>,
        #[arg(long)]
        ndk: Option<std::path::PathBuf>,
        #[arg(long, default_value_t = 21)]
        android_api: u32,
    },
    /// Search a registry for packages by name.
    Search { query: String },
    /// Print the resolved dependency tree from dream.lock.
    Tree {
        #[arg(short = 'p', long = "package", value_name = "NAME")]
        package: Option<String>,
    },
    /// Install or manage pinned build tools (Binaryen, Slang, …) under ~/.dream/toolchains/.
    Toolchain {
        #[command(subcommand)]
        cmd: ToolchainCmd,
    },
}

#[derive(Subcommand)]
enum ToolchainCmd {
    /// Diagnose the compiler toolchain without installing components.
    Doctor {
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Download pinned build tools for this OS/arch. `dream` installs missing tools when needed.
    Install {
        /// A pinned tool such as `binaryen` (WASM optimizer) or `slang` (shader compiler). Omit
        /// to install the default set (`binaryen`).
        component: Option<String>,
    },
    /// Show which toolchain components are installed.
    List,
    /// Remove an installed pinned tool (`binaryen`, `slang`, …).
    Uninstall { component: String },
}

fn print_error(msg: impl Display) {
    eprintln!("{} {}", Paint::red("error:").bold(), Paint::new(msg).bold());
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let cwd = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            print_error(format!("could not determine current directory: {e}"));
            return ExitCode::FAILURE;
        }
    };

    let result = match cli.command {
        Cmd::Devices { target, json } => dreamer::mobile::devices(target.as_deref(), json),
        Cmd::Init(args) => args.run(&cwd),
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
        Cmd::Build {
            opt,
            package,
            target,
        } => match opt.into_compile_flags() {
            Ok(flags) => {
                commands::build::run_with(&cwd, flags, package.as_deref(), target.as_deref())
            }
            Err(e) => {
                print_error(e);
                return ExitCode::FAILURE;
            }
        },
        Cmd::Run {
            target,
            device,
            opt,
            port,
            package,
            args,
        } => match opt.into_compile_flags() {
            Ok(flags) => commands::run::run_with(
                &cwd,
                commands::run::Selection { target, device },
                flags,
                port,
                &args,
                package.as_deref(),
            ),
            Err(e) => {
                print_error(e);
                return ExitCode::FAILURE;
            }
        },
        Cmd::Test {
            opt,
            filter,
            package,
            native,
            node,
        } => match opt.into_compile_flags() {
            Ok(flags) => commands::test::run_with(
                &cwd,
                flags,
                filter,
                package.as_deref(),
                commands::test::Hosts { native, node },
            ),
            Err(e) => {
                print_error(e);
                return ExitCode::FAILURE;
            }
        },
        Cmd::Publish {
            registry,
            token,
            package,
        } => commands::publish::run(&cwd, registry, token, package.as_deref()),
        Cmd::Pack {
            opt,
            targets,
            package,
            slices,
            android_package,
            ndk,
            android_api,
        } => {
            if targets.len() == 1 && matches!(targets[0].as_str(), "ios" | "android") {
                if opt.release
                    || opt.optimize.is_some()
                    || opt.wasm
                    || opt.profile
                    || opt.use_profile.is_some()
                {
                    print_error(
                        "mobile pack consumes target-built libraries; compilation flags do not apply",
                    );
                    return ExitCode::FAILURE;
                }
                return match commands::pack::mobile::run(
                    &cwd,
                    package.as_deref(),
                    commands::pack::mobile::Options {
                        platform: targets[0].clone(),
                        slices,
                        android_package,
                        ndk,
                        android_api,
                    },
                ) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(error) => {
                        print_error(error);
                        ExitCode::FAILURE
                    }
                };
            }
            if !slices.is_empty() || android_package.is_some() || ndk.is_some() {
                print_error("--slice, --android-package and --ndk require --target ios or android");
                return ExitCode::FAILURE;
            }
            if opt.profile || opt.use_profile.is_some() {
                print_error("pack does not support --profile / --use-profile");
                return ExitCode::FAILURE;
            }
            match CompileFlags::for_pack(opt.optimize, opt.wasm) {
                Ok(flags) => commands::pack::run(&cwd, &targets, package.as_deref(), flags),
                Err(e) => {
                    print_error(e);
                    return ExitCode::FAILURE;
                }
            }
        }
        Cmd::Search { query } => commands::search::run(&cwd, &query),
        Cmd::Tree { package } => commands::tree::run(&cwd, package.as_deref()),
        Cmd::Toolchain { cmd } => match cmd {
            ToolchainCmd::Install { component } => commands::toolchain::install(component),
            ToolchainCmd::Doctor { target, json } => {
                commands::toolchain::doctor(target.as_deref(), json)
            }
            ToolchainCmd::List => commands::toolchain::list(),
            ToolchainCmd::Uninstall { component } => commands::toolchain::uninstall(component),
        },
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            print_error(format!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}
