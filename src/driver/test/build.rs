//! Compiles a project's runner for one host into `target/test/<profile|node>/`, reusing the build
//! cache, and answers the command that runs it from that directory.

use super::project::{TestHost, TestProject};
use super::runner::RunnerPlan;
use crate::driver::compiler::{BuildOutcome, Compiler};
use crate::driver::js_runtime::JsRuntimeTarget;
use crate::driver::output::OutputKind;
use crate::driver::wasm_opt::OptLevel;
use dream_abi::attributes::CompileTargets;
use dream_sema::analyzer::CrateType;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

type Config = Arc<crate::driver::toolchain::ToolchainConfig>;

pub struct BuildRequest<'a> {
    pub config: &'a Config,
    pub project: &'a TestProject,
    pub plan: &'a RunnerPlan,
    pub release: bool,
    pub optimize: Option<OptLevel>,
}

impl BuildRequest<'_> {
    pub fn build(&self, host: TestHost) -> Result<Command, String> {
        let out_dir = self.out_dir(host);
        std::fs::create_dir_all(&out_dir)
            .map_err(|e| format!("creating {}: {e}", out_dir.display()))?;
        crate::driver::assets::sync(
            &self.project.asset_root,
            &self.project.assets,
            &out_dir,
        )?;
        let out_dir = std::path::absolute(&out_dir).map_err(|e| e.to_string())?;
        match host {
            TestHost::Native => self.build_native(&out_dir),
            TestHost::Node => self.build_node(&out_dir),
        }
    }

    fn out_dir(&self, host: TestHost) -> PathBuf {
        let sub = match host {
            TestHost::Native if self.release => "release",
            TestHost::Native => "debug",
            TestHost::Node => "node",
        };
        self.project.root.join("target").join("test").join(sub)
    }

    fn compiler(&self, wasm: bool, cache_key: String) -> Result<Compiler, String> {
        let target = crate::driver::target::resolve(wasm, None)?;
        let mut compiler = Compiler::new_with_toolchain_config(target, self.config.clone())
            .with_virtual_entry(self.plan.source())
            .with_build_cache(Some(cache_key))
            .with_release(self.release)
            .with_crate_type(CrateType::Bin);
        if let Some(level) = self.optimize {
            compiler = compiler.with_optimize(Some(level));
        }
        Ok(compiler)
    }

    fn compile(&self, compiler: Compiler, out: &Path) -> Result<BuildOutcome, String> {
        let entry = RunnerPlan::entry_path(&self.project.root);
        compiler
            .compile(&entry, &out.to_string_lossy())
            .map_err(|e| match e.to_string() {
                detail if detail.trim().is_empty() => {
                    format!("test project '{}' did not compile", self.project.name)
                }
                detail => format!("compiling test project '{}': {detail}", self.project.name),
            })
    }

    fn build_native(&self, out_dir: &Path) -> Result<Command, String> {
        let ll = out_dir.join("runner.ll");
        let _lock = crate::driver::output::acquire_lock(&ll).map_err(|e| e.to_string())?;
        let opt = OptLevel::from_cli(self.release, self.optimize);
        let spec = dream_abi::target::TargetSpec::host();
        let compiler = self
            .compiler(false, format!("test|native|{}", opt.as_cli_flag()))?
            .with_raw_ll_intermediate(true);
        let bin = OutputKind::Executable.artifact_path(&ll, &spec);
        if let BuildOutcome::Built(stamp) = self.compile(compiler, &ll)? {
            crate::execution::llvm::compile_llvm(
                self.config,
                &ll,
                crate::execution::llvm::NativeBuildOptions {
                    target: spec,
                    opt_ll: None,
                    opt_remarks: None,
                    opt,
                    debug: false,
                    profile: dream_abi::profile::CompileProfile::from_release(self.release),
                    pgo: &crate::execution::native::Pgo::Off,
                    icon: None,
                    relocatable: false,
                    output_kind: OutputKind::Executable,
                },
            )
            .map_err(|e| format!("linking the {} runner: {e}", self.project.name))?;
            let _ = std::fs::remove_file(&ll);
            let _ = std::fs::remove_file(crate::driver::compiler::c_shim_path(&ll));
            if let Some(stamp) = stamp {
                stamp.store(&existing(&[bin.clone(), ll.with_extension("abi.json")]));
            }
        }
        crate::execution::native::native_command(
            self.config,
            &bin,
            &ll.to_string_lossy(),
            Some(out_dir),
        )
        .map_err(|e| e.to_string())
    }

    fn build_node(&self, out_dir: &Path) -> Result<Command, String> {
        let wat = out_dir.join("runner.wat");
        let _lock = crate::driver::output::acquire_lock(&wat).map_err(|e| e.to_string())?;
        let compiler = self
            .compiler(true, "test|node".into())?
            .with_runtimes(vec![JsRuntimeTarget::Node])
            .with_compile_targets(CompileTargets {
                native: false,
                node: true,
                web: false,
            })
            .with_output_kind(OutputKind::Wasm);
        let wasm = wat.with_extension("wasm");
        let js = out_dir.join(format!("runner.{}", JsRuntimeTarget::Node.runtime_extension()));
        if let BuildOutcome::Built(Some(stamp)) = self.compile(compiler, &wat)? {
            stamp.store(&existing(&[
                wasm.clone(),
                js.clone(),
                wat.with_extension("abi.json"),
            ]));
        }
        let script = out_dir.join("runner.mjs");
        let source = format!(
            "import {{ run }} from \"./{}\";\nawait run(\"./runner.wasm\", {{ leakReport: true }});\n",
            js.file_name().unwrap_or_default().to_string_lossy()
        );
        if std::fs::read_to_string(&script).ok().as_deref() != Some(source.as_str()) {
            std::fs::write(&script, source)
                .map_err(|e| format!("writing {}: {e}", script.display()))?;
        }
        let mut cmd = Command::new("node");
        cmd.arg(&script).current_dir(out_dir);
        Ok(cmd)
    }
}

fn existing(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths.iter().filter(|p| p.is_file()).cloned().collect()
}
