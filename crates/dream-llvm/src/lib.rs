//! MIR → LLVM IR (text) + clang driver.
//!
//! Retain/release stay opaque runtime calls. Guest pointers are `i32` heap offsets.

mod clang;
mod debug_map;
mod emit;
mod options;
mod triple;

pub use clang::{compile_ir, shared_lib_ext, ClangError};
pub use debug_map::debug_map_json;
pub use emit::emit_module_ir;
pub use options::{CodegenOptions, Lto, Sanitize};
pub use triple::{host_triple, Triple};

use dream_mir::Mir;
use dream_types::TypeInterner;

/// Full LLVM IR module text for `mir`.
pub fn emit_ir(mir: &Mir, interner: &TypeInterner, opts: &CodegenOptions) -> String {
    emit_module_ir(mir, interner, opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dream_mir::Mir;

    #[test]
    fn empty_module_is_valid_ir_prefix() {
        let mir = Mir::default();
        let intern = dream_types::TypeInterner::new();
        let ir = emit_ir(&mir, &intern, &CodegenOptions::default());
        assert!(ir.contains("target triple"));
        assert!(ir.contains("declare i32 @dream_malloc"));
    }
}
