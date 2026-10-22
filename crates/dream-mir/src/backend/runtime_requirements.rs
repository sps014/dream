//! Startup services derived after MIR pruning. Foreign code and exported entry points can
//! invoke the embedding contract, so their requirements remain conservative.

use crate::Mir;
use dream_abi::intrinsics::{IntrinsicOp, is_llvm_intrinsic_key};
use dream_abi::{js_abi, runtime_hosts};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeRequirements {
    pub embedding: bool,
    pub heap: bool,
    pub callbacks: bool,
    pub deferred_cleanup: bool,
    pub process_arguments: bool,
}

impl RuntimeRequirements {
    pub fn from_mir(mir: &Mir, interner: &dream_types::TypeInterner) -> Self {
        let embedding = !mir.exports.is_empty()
            || mir
                .imports
                .iter()
                .any(super::shared::abi_types::is_c_import);
        let opaque_intrinsics = mir
            .intrinsics
            .iter()
            .any(|(_, key)| IntrinsicOp::from_key(key).is_none() && !is_llvm_intrinsic_key(key));
        let callbacks = embedding
            || opaque_intrinsics
            || mir.intrinsics.iter().any(|(_, key)| {
                matches!(
                    IntrinsicOp::from_key(key),
                    Some(IntrinsicOp::CallbackRegister | IntrinsicOp::CallbackUnregister)
                )
            })
            || !mir.polls.is_empty()
            || mir.functions.iter().any(|f| f.is_async)
            || mir.imports.iter().any(|i| {
                !(i.module == js_abi::HOST_MODULE
                    && [runtime_hosts::PROCESS_ARGS, runtime_hosts::PROCESS_EXE_PATH]
                        .contains(&i.field.as_str()))
            });
        let process_arguments = embedding
            || opaque_intrinsics
            || mir.imports.iter().any(|i| {
                i.module == js_abi::HOST_MODULE
                    && [runtime_hosts::PROCESS_ARGS, runtime_hosts::PROCESS_EXE_PATH]
                        .contains(&i.field.as_str())
            });
        let heap = super::heap_requirements::required(mir, interner, embedding);
        let deferred_cleanup = embedding || opaque_intrinsics || mir.uses_defer;
        tracing::debug!(
            embedding,
            heap,
            callbacks,
            deferred_cleanup,
            process_arguments,
            "reachable runtime startup services"
        );
        Self {
            embedding,
            heap,
            callbacks,
            deferred_cleanup,
            process_arguments,
        }
    }
}
