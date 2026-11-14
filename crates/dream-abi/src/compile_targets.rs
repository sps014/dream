//! Which hosts one compile runs on. A WebAssembly module is built for the browser, for Node.js,
//! or for both at once (`--runtime --web --node`), and `@cfg` sees the matching symbols.

use crate::target::{ExecutionEnvironment, TargetSpec};
use dream_syntax::conditional::{CfgSymbols, TargetSymbol};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompileTargets {
    pub native: bool,
    pub node: bool,
    pub web: bool,
}

impl CompileTargets {
    pub fn native_only() -> Self {
        Self {
            native: true,
            node: false,
            web: false,
        }
    }

    /// The `@cfg` symbols of a build of `spec` for these hosts. A JavaScript-hosted module
    /// defines `Js`, plus `Browser` or `Node` for its one host; a module for both defines
    /// neither, so only declarations that work on both hosts are visible.
    pub fn cfg_symbols(self, spec: &TargetSpec, release: bool) -> CfgSymbols {
        let symbols = spec.cfg_symbols(release);
        if spec.execution_environment != ExecutionEnvironment::Browser || !self.node {
            return symbols;
        }
        let symbols = symbols.without(TargetSymbol::Browser);
        if self.web {
            symbols
        } else {
            symbols.with(TargetSymbol::Node)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn javascript_hosts_select_browser_or_node() {
        let spec = TargetSpec::parse("wasm32-unknown-unknown").unwrap();
        let key = |node, web| {
            CompileTargets {
                native: false,
                node,
                web,
            }
            .cfg_symbols(&spec, false)
            .key()
        };
        assert_eq!(key(false, true), "Debug,Wasm,Js,Browser");
        assert_eq!(key(true, false), "Debug,Wasm,Js,Node");
        assert_eq!(key(true, true), "Debug,Wasm,Js");
    }
}
