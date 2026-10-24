//! Whole-program strong-cycle check over typed HIR.
//!
//! A class only needs `@allow_cycle` when the program contains a store that may really close a
//! strong cycle. Layouts give a cheap prefilter (only stores inside a cyclic type component can
//! close one); a flow-sensitive walk with bottom-up call summaries then proves each such store
//! safe or reports the strong field the cycle runs through. Annotations never change runtime ARC.

use super::*;
use dream_hir::{HFunction, HGlobal, InterfaceTable, LayoutTable};
use dream_types::DefId;
use indexmap::IndexSet;

mod contents;
mod exprs;
mod instantiate;
mod managed;
mod program;
mod report;
mod state;
mod stmts;
mod store;
mod summary;
mod type_graph;
mod value;
mod visit;
mod walker;

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn validate_cycles(
        &self,
        node: &'a ProgramView<'a>,
        layouts: &LayoutTable,
        functions: &[HFunction],
        globals: &[HGlobal],
        interfaces: &InterfaceTable,
        diagnostics: &mut DiagnosticBag,
    ) {
        let _phase = tracing::info_span!("compile_phase", phase = "cycle_analysis").entered();
        let graph = type_graph::TypeGraph::build(&self.type_ctx.interner, layouts, interfaces);
        if !graph.any_cyclic {
            return;
        }
        let intrinsics = || {
            self.function_table.functions.iter().filter_map(|((def, _), info)| {
                let op = info
                    .intrinsic_name
                    .as_deref()
                    .and_then(dream_abi::intrinsics::IntrinsicOp::from_key)?;
                Some((*def, op))
            })
        };
        let builtins = program::Builtins {
            funcbox_new: self.closure_intrinsic("funcbox_new"),
            funcbox_env: self.closure_intrinsic("funcbox_env"),
            funcbox_funcidx: self.closure_intrinsic("funcbox_funcidx"),
            future: self
                .type_ctx
                .resolve(DefKind::Struct, dream_syntax::nodes::types::FUTURE_TYPE),
            inspecting: intrinsics()
                .filter(|&(_, op)| only_inspects(op))
                .map(|(def, _)| def)
                .collect(),
            allocating: intrinsics()
                .filter(|&(_, op)| only_allocates(op))
                .map(|(def, _)| def)
                .collect(),
            intrinsics: intrinsics()
                .map(|(def, _)| def)
                .chain(self.function_table.functions.iter().filter_map(|((def, _), info)| {
                    // `dream_start` only enqueues the future; it runs once this task suspends.
                    (info.intrinsic_name.as_deref() == Some("dream_start")).then_some(*def)
                }))
                .collect(),
        };
        let exports: IndexSet<DefId> = node
            .functions
            .iter()
            .filter(|f| dream_abi::attributes::has_export_attr(&f.attributes))
            .filter_map(|f| self.function_table.declaration_node(f).map(|(def, _)| def))
            .collect();
        let managed: IndexSet<DefId> = node
            .structs
            .iter()
            .filter(|d| dream_abi::attributes::allows_cycle(&d.attributes))
            .filter_map(|d| {
                let scope = self.graph.module_for_file(d.file_path.as_deref());
                self.type_ctx
                    .resolve_from(scope, DefKind::Struct, &d.name.text)
            })
            .collect();
        let mut program =
            program::Program::new(graph, functions, interfaces, builtins, managed);
        let findings = program::run(&mut program, globals, &exports);
        self.report_cycles(node, &program.graph, findings, diagnostics);
    }
}

/// Intrinsics that read their arguments and keep no reference to them.
fn only_inspects(op: dream_abi::intrinsics::IntrinsicOp) -> bool {
    use dream_abi::intrinsics::IntrinsicOp as Op;
    matches!(
        op,
        Op::DebugRefCount
            | Op::DebugFreeList
            | Op::DebugHeapPtr
            | Op::DebugLiveObjects
            | Op::DebugTotalAllocations
            | Op::DebugRuntimeCounter
            | Op::Print
            | Op::Println
            | Op::Panic
    )
}

/// Intrinsics that return a new zero-initialized value and keep no reference to their arguments.
fn only_allocates(op: dream_abi::intrinsics::IntrinsicOp) -> bool {
    use dream_abi::intrinsics::IntrinsicOp as Op;
    matches!(op, Op::ArrayNew | Op::BufferZero | Op::YieldSignal)
}
