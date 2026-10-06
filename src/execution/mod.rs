//! Native execution of compiled Dream modules: LLVM-built guest + capability hosts (`native`), DAP via lldb.

pub mod debugger;
pub mod llvm;
pub mod native;
