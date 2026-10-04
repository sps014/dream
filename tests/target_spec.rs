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

#[test]
fn pointer_integer_literal_ranges_follow_the_selected_target() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("literal.dream");
    let output = temporary.path().join("literal.ll");
    for (ty, value) in [("isize", "2147483648"), ("usize", "4294967296")] {
        std::fs::write(
            &source,
            format!("fun main(): void {{ let n: {ty} = {value}; }}"),
        )
        .unwrap();
        for (triple, fits) in [
            ("i686-unknown-linux-gnu", false),
            ("x86_64-unknown-linux-gnu", true),
        ] {
            let capture = Arc::new(CaptureTarget(Mutex::new(None)));
            let error = Compiler::new(Target::Llvm(TargetSpec::parse(triple).unwrap()))
                .with_llvm(capture.clone())
                .compile(
                    &source.to_string_lossy().into_owned(),
                    &output.to_string_lossy(),
                )
                .unwrap_err();
            if fits {
                assert!(
                    error
                        .to_string()
                        .contains("target captured before loading runtime"),
                    "{}",
                    error
                );
                assert!(capture.0.lock().unwrap().is_some());
            } else {
                assert!(
                    matches!(error, dream::driver::error::CompileError::Semantic(_)),
                    "{}",
                    error
                );
                assert!(capture.0.lock().unwrap().is_none());
            }
        }
    }
}

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
                &output.to_string_lossy(),
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
            &output.to_string_lossy(),
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

#[test]
#[cfg(feature = "native")]
fn mobile_library_objects_have_the_selected_architecture_and_typed_exports() {
    for (triple, architecture, format) in [
        (
            "arm64-apple-ios",
            object::Architecture::Aarch64,
            object::BinaryFormat::MachO,
        ),
        (
            "arm64-apple-ios-simulator",
            object::Architecture::Aarch64,
            object::BinaryFormat::MachO,
        ),
        (
            "aarch64-linux-android",
            object::Architecture::Aarch64,
            object::BinaryFormat::Elf,
        ),
        (
            "x86_64-linux-android",
            object::Architecture::X86_64,
            object::BinaryFormat::Elf,
        ),
    ] {
        use object::Object;
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("dream.toml"),
            "[package]\nname = \"mobile\"\ntype = \"lib\"\n[lib]\noutput-type = \"staticlib\"\n",
        )
        .unwrap();
        let source = directory.path().join("lib.dream");
        let ir = directory.path().join("lib.ll");
        std::fs::write(
            &source,
            "@export fun add(a: int, b: int): int { return a + b; }",
        )
        .unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_dream"))
            .args(["--object", "--target", triple, "-o"])
            .arg(&ir)
            .arg(source)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let bytes = std::fs::read(ir.with_extension("o")).unwrap();
        let object = object::File::parse(bytes.as_slice()).unwrap();
        assert_eq!(object.architecture(), architecture);
        assert_eq!(object.format(), format);
        assert!(std::fs::read_to_string(ir.with_extension("h"))
            .unwrap()
            .contains("int32_t add(int32_t arg0, int32_t arg1)"));
        let abi: serde_json::Value =
            serde_json::from_slice(&std::fs::read(ir.with_extension("abi.json")).unwrap()).unwrap();
        assert_eq!(abi["export_functions"][0]["name"], "add");
        assert_eq!(abi["export_functions"][0]["ret"]["kind"], "Int");
    }
}

#[test]
#[cfg(feature = "native")]
fn toolchain_doctor_reports_missing_foreign_libraries_without_host_fallback() {
    let directory = tempfile::tempdir().unwrap();
    let targets = directory.path().join("foreign");
    let triple = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "aarch64-unknown-linux-gnu"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_dream"))
        .args(["toolchain-doctor", "--target", triple, "--json"])
        .env("DREAM_TARGETS", &targets)
        .env("DREAM_NO_AUTO_INSTALL", "1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["healthy"], false);
    assert_eq!(report["target"], triple);
    assert_eq!(
        report["paths"]["targets"],
        targets.to_string_lossy().as_ref()
    );
    assert_eq!(report["tools"]["dream_host_core"]["available"], false);
    assert!(report["configuration_hash"].as_str().unwrap().len() == 64);
    assert!(!targets.exists());
}
