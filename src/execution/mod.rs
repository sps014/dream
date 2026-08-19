//! Native execution of compiled Dream modules: C guest + libdream host (`native_c`), DAP via lldb.

pub mod debugger;
pub mod host;
pub mod native_c;
