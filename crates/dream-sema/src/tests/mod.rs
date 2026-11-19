//! Analyzer test suite (analysis/diagnostics only).
//! HIR→MIR→LLVM IR emission lives in the root crate's `tests/sema_emission_tests.rs`.

mod analysis_tests;
mod diagnostic_cascades;
mod harness;
mod value_classification;

mod access;
