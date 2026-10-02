use dream::driver::compiler::{Compiler, LlvmRuntimeRequest, LlvmToolchain};
use dream_abi::target::TargetSpec;
use dream_mir::backend::Target;
use std::path::Path;
use std::sync::{Arc, Mutex};

struct CaptureTarget(Mutex<Option<TargetSpec>>);

impl LlvmToolchain for CaptureTarget {
    fn runtime_sigs(&self, req: &LlvmRuntimeRequest) -> Result<String, String> {
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

#[test]
fn selected_target_reaches_runtime_loading_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("main.dream");
    let output = temp.path().join("main.ll");
    std::fs::write(&source, "fun main(): void {}").unwrap();
    for triple in [
        "i686-unknown-linux-gnu",
        "aarch64-apple-macosx13.2.0",
        "wasm32-unknown-wasi",
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
