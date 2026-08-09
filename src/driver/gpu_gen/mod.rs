//! Dream `@compute` / `@vertex` / `@fragment` → WGSL emitter.
//!
//! Shader bodies never enter MIR/WAT. This pass walks the AST of every GPU-stage function and
//! emits WGSL text plus binding metadata for the `.abi.json` `"gpu"` section / `.wgsl` sidecar.

mod compute;
mod context;
mod expr;
mod fragment;
mod ident;
mod layout;
mod output;
mod stmt;
mod ty;
mod types;
mod validate;
mod vertex;

pub use output::{gpu_abi_json, join_wgsl_module};
pub use types::{GpuBinding, GpuEmitResult, GpuKernelInfo, GpuShaderInfo, GpuVertexAttr};

use dream_abi::attributes::{has_compute_attr, has_fragment_attr, has_vertex_attr};
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::ProgramNode;

/// Emit WGSL for every `@compute` / `@vertex` / `@fragment` function in `program`.
///
/// After emission, each shader/kernel source is parsed and validated with naga so reserved
/// identifiers, type errors, and other WGSL issues fail at compile time instead of in the browser.
pub fn collect_gpu_shaders(
    program: &ProgramNode<'_>,
    diagnostics: &mut DiagnosticBag,
) -> GpuEmitResult {
    let mut out = GpuEmitResult::default();
    for func in &program.functions {
        let saved = diagnostics.file_path.clone();
        if let Some(path) = &func.file_path {
            diagnostics.file_path = Some(path.to_string());
        }
        if has_compute_attr(&func.attributes) {
            out.kernels.push(compute::emit_kernel(func, diagnostics));
        } else if has_vertex_attr(&func.attributes) {
            out.shaders
                .push(vertex::emit_vertex(func, program, diagnostics));
        } else if has_fragment_attr(&func.attributes) {
            out.shaders
                .push(fragment::emit_fragment(func, program, diagnostics));
        }
        diagnostics.file_path = saved;
    }
    if !diagnostics.has_errors() {
        validate::validate_gpu_wgsl(&out, diagnostics);
    }
    out
}
