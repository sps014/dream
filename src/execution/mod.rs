//! Native execution of compiled Dream modules: LLVM-built guest + libdream host (`native`), DAP via lldb.

pub mod debugger;
pub mod native;
pub mod llvm;
