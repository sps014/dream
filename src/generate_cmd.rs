//! `dream generate` and the generator debugging entry points (`dream debug-adapter
//! --generator`, `dream build --debug-generator`).

use dream::driver::compiler::Compiler;
use dream::driver::generate::commands::{self, DebugLaunch};
use dream::driver::toolchain::ToolchainConfig;
use dream::driver::ui::Ui;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

pub enum GenerateAction {
    List,
    Explain(String),
    Capture(String, Option<PathBuf>),
    VerifyIncremental,
    Prewarm,
}

fn inspect(
    ui: &Ui,
    compiler: &Compiler,
    entry: &str,
) -> Option<dream::driver::generate::GenInspection> {
    match compiler.inspect_generators(entry) {
        Ok(insp) => Some(insp),
        Err(e) => {
            // The compiler already rendered diagnostic failures.
            if e.diagnostic_text().is_none() {
                ui.error(&e.to_string());
            }
            None
        }
    }
}

fn finish(ui: &Ui, result: Result<String, String>) -> ExitCode {
    match result {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            ui.error(&e);
            ExitCode::FAILURE
        }
    }
}

pub fn run_generate(
    ui: &Ui,
    config: &Arc<ToolchainConfig>,
    compiler: &Compiler,
    entry: &str,
    action: GenerateAction,
) -> ExitCode {
    let Some(insp) = inspect(ui, compiler, entry) else {
        return ExitCode::FAILURE;
    };
    let result = match action {
        GenerateAction::List => Ok(commands::list(&insp)),
        GenerateAction::Explain(gen_name) => commands::explain(config, &insp, &gen_name),
        GenerateAction::Capture(gen_name, dir) => {
            let dir = dir.unwrap_or_else(|| commands::default_capture_dir(entry, &gen_name));
            commands::capture(config, &insp, &gen_name, &dir).map(|text| {
                format!(
                    "{text}debug it with `dream debug-adapter --generator {gen_name} --snapshot {}`\n",
                    dir.display()
                )
            })
        }
        GenerateAction::VerifyIncremental => commands::verify_incremental(config, &insp),
        GenerateAction::Prewarm => commands::prewarm(config, &insp),
    };
    finish(ui, result)
}

/// `dream generate --replay <dir>`: needs no source file.
pub fn replay(ui: &Ui, config: &Arc<ToolchainConfig>, dir: &Path) -> ExitCode {
    finish(ui, commands::replay(config, dir))
}

/// Serves one DAP session on a prepared launch. DAP owns stdout, so the result summary goes to
/// stderr.
fn serve(ui: &Ui, config: &Arc<ToolchainConfig>, launch: Result<DebugLaunch, String>) -> ExitCode {
    let launch = match launch {
        Ok(launch) => launch,
        Err(e) => {
            ui.error(&e);
            return ExitCode::FAILURE;
        }
    };
    let module = launch.exe.ll.to_string_lossy().into_owned();
    if let Err(e) = dream::execution::debugger::run_debug_adapter(
        config,
        &launch.exe.bin,
        &module,
        &launch.exe.ll,
        &launch.args,
    ) {
        ui.error(&format!("debug adapter failed: {e}"));
        return ExitCode::FAILURE;
    }
    eprint!("{}", commands::debug_summary(&launch));
    ExitCode::SUCCESS
}

/// Debugs a generator on the program's current snapshot (or on `snapshot`, a snapshot file).
pub fn debug_generator(
    ui: &Ui,
    config: &Arc<ToolchainConfig>,
    compiler: &Compiler,
    entry: &str,
    generator: &str,
    snapshot: Option<&Path>,
) -> ExitCode {
    let Some(insp) = inspect(ui, compiler, entry) else {
        return ExitCode::FAILURE;
    };
    serve(
        ui,
        config,
        commands::debug_launch(config, &insp, generator, snapshot),
    )
}

/// Debugs a capture directory: no source file or front end needed.
pub fn debug_capture(
    ui: &Ui,
    config: &Arc<ToolchainConfig>,
    dir: &Path,
    generator: &str,
) -> ExitCode {
    serve(
        ui,
        config,
        commands::debug_launch_from_capture(config, dir, generator),
    )
}
