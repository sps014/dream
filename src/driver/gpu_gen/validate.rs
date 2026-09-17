//! Compile-time WGSL validation via naga (parse + full module validation).

use super::output::join_wgsl_module;
use super::types::GpuEmitResult;
use dream_diagnostics::DiagnosticBag;

/// Validate every emitted kernel/shader source with naga before the sidecar is written.
///
/// Each entry is checked on its own (matching the browser host, which creates one
/// `GPUShaderModule` per stage from `abi.gpu.*.source`). Failures become generator diagnostics.
pub(super) fn validate_gpu_wgsl(result: &GpuEmitResult, diagnostics: &mut DiagnosticBag) {
    if result.is_empty() {
        return;
    }
    for k in &result.kernels {
        validate_one(&k.wgsl, &format!("@compute '{}'", k.name), diagnostics);
    }
    for sh in &result.shaders {
        validate_one(
            &sh.wgsl,
            &format!("@{} '{}'", sh.stage, sh.name),
            diagnostics,
        );
    }
    // The `.wgsl` sidecar joins every entry into one module, and `join_wgsl_module` folds the
    // declarations stages share so that module is legal WGSL. Checking it only after the
    // per-entry checks pass keeps a single broken shader from being reported twice.
    if !diagnostics.has_errors() {
        let joined = join_wgsl_module(result);
        validate_one(&joined, "joined GPU WGSL module", diagnostics);
    }
}

fn validate_one(source: &str, label: &str, diagnostics: &mut DiagnosticBag) {
    let module = match naga::front::wgsl::parse_str(source) {
        Ok(m) => m,
        Err(e) => {
            diagnostics.report_error(
                format!(
                    "invalid WGSL emitted for {label}:\n{}",
                    e.emit_to_string(source)
                ),
                None,
            );
            return;
        }
    };
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    if let Err(e) = validator.validate(&module) {
        diagnostics.report_error(
            format!(
                "invalid WGSL emitted for {label}:\n{}",
                e.emit_to_string(source)
            ),
            None,
        );
    }
}
