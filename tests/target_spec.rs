use dream::driver::compiler::{Compiler, LlvmRuntimeRequest, LlvmToolchain, RuntimeSignatures};
use dream_abi::target::TargetSpec;
use dream_mir::backend::Target;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

struct CaptureTarget(Mutex<Option<TargetSpec>>);

impl LlvmToolchain for CaptureTarget {
    fn runtime_sigs(&self, req: &LlvmRuntimeRequest) -> Result<RuntimeSignatures, String> {
        *self.0.lock().unwrap() = Some(req.target.spec().clone());
        Err("target captured before loading runtime".into())
    }

    fn link_wasm(
        &self,
        _: &Path,
        _: &Path,
        _: Option<&Path>,
        _: &LlvmRuntimeRequest,
    ) -> Result<(), String> {
        unreachable!("signature loading fails before linking")
    }
}

struct StaleRuntime;

impl LlvmToolchain for StaleRuntime {
    fn runtime_sigs(&self, req: &LlvmRuntimeRequest) -> Result<RuntimeSignatures, String> {
        let data_layout = if req.target.spec().ptr_size == 4 {
            "e-p:32:32"
        } else {
            "e"
        };
        Ok(RuntimeSignatures {
            text: format!(
                "target datalayout = \"{data_layout}\"\ntarget triple = \"{}\"\n",
                req.target.spec().triple
            ),
            cache_path: PathBuf::from("/stale-cache/dream_rt.sigs"),
        })
    }

    fn link_wasm(
        &self,
        _: &Path,
        _: &Path,
        _: Option<&Path>,
        _: &LlvmRuntimeRequest,
    ) -> Result<(), String> {
        unreachable!("signature loading fails before linking")
    }
}

#[test]
fn selected_target_reaches_runtime_loading_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("main.dream");
    let output = temp.path().join("main.ll");
    std::fs::write(&source, "fun main(): void {}").unwrap();
    for triple in [
        "i686-unknown-linux-gnu",
        "aarch64-apple-macosx13.2.0",
        "wasm32-unknown-wasip1",
    ] {
        let spec = TargetSpec::parse(triple).unwrap();
        let capture = Arc::new(CaptureTarget(Mutex::new(None)));
        let error = Compiler::new(Target::Llvm(spec.clone()))
            .with_llvm(capture.clone())
            .compile(
                &source.to_string_lossy().into_owned(),
                &output.to_string_lossy().into_owned(),
            )
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("target captured before loading runtime"));
        assert_eq!(*capture.0.lock().unwrap(), Some(spec));
    }
}

#[test]
fn stale_runtime_symbol_is_a_toolchain_error_with_the_cache_path() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("main.dream");
    let output = temp.path().join("main.ll");
    std::fs::write(&source, "fun main(): void {}").unwrap();
    let error = Compiler::new(Target::native())
        .with_llvm(Arc::new(StaleRuntime))
        .compile(
            &source.to_string_lossy().into_owned(),
            &output.to_string_lossy().into_owned(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        dream::driver::error::CompileError::Toolchain(_)
    ));
    let message = error.to_string();
    assert!(
        message.contains("/stale-cache/dream_rt.sigs"),
        "{}",
        message
    );
    assert!(message.contains("required runtime symbol"), "{}", message);
}
