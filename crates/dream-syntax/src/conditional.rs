//! The fixed set of names a `@cfg(...)` condition can test. A build defines a subset of these
//! symbols; the parser drops every item whose condition is false before analysis sees it.

/// The attribute that gates a declaration, member, or statement on the build target.
pub const CFG_ATTRIBUTE: &str = "cfg";

/// The namespace every condition symbol is spelled under (`Target.Windows`). It names nothing
/// outside a `@cfg` condition.
pub const TARGET_NAMESPACE: &str = "Target";

/// One name a `@cfg` condition can test, spelled `Target.<name>` in source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TargetSymbol {
    Debug,
    Release,
    Windows,
    Macos,
    Linux,
    Ios,
    Native,
    Wasm,
    Browser,
    Wasi,
}

impl TargetSymbol {
    pub const ALL: [TargetSymbol; 10] = [
        TargetSymbol::Debug,
        TargetSymbol::Release,
        TargetSymbol::Windows,
        TargetSymbol::Macos,
        TargetSymbol::Linux,
        TargetSymbol::Ios,
        TargetSymbol::Native,
        TargetSymbol::Wasm,
        TargetSymbol::Browser,
        TargetSymbol::Wasi,
    ];

    /// The member name after `Target.`.
    pub fn name(self) -> &'static str {
        match self {
            TargetSymbol::Debug => "Debug",
            TargetSymbol::Release => "Release",
            TargetSymbol::Windows => "Windows",
            TargetSymbol::Macos => "Macos",
            TargetSymbol::Linux => "Linux",
            TargetSymbol::Ios => "Ios",
            TargetSymbol::Native => "Native",
            TargetSymbol::Wasm => "Wasm",
            TargetSymbol::Browser => "Browser",
            TargetSymbol::Wasi => "Wasi",
        }
    }

    pub fn from_name(name: &str) -> Option<TargetSymbol> {
        Self::ALL.into_iter().find(|symbol| symbol.name() == name)
    }

    fn bit(self) -> u16 {
        1 << self as u16
    }
}

/// The symbols one build defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CfgSymbols {
    bits: u16,
}

impl CfgSymbols {
    pub const fn empty() -> CfgSymbols {
        CfgSymbols { bits: 0 }
    }

    pub fn with(mut self, symbol: TargetSymbol) -> CfgSymbols {
        self.bits |= symbol.bit();
        self
    }

    pub fn defines(self, symbol: TargetSymbol) -> bool {
        self.bits & symbol.bit() != 0
    }

    /// A debug build for the machine running the compiler. Editors and tools without a selected
    /// target use this set; inside a browser (the playground) that is a browser wasm build.
    pub fn host_debug() -> CfgSymbols {
        let debug = CfgSymbols::empty().with(TargetSymbol::Debug);
        if cfg!(target_family = "wasm") {
            return debug.with(TargetSymbol::Wasm).with(TargetSymbol::Browser);
        }
        let symbols = debug.with(TargetSymbol::Native);
        if cfg!(target_os = "windows") {
            symbols.with(TargetSymbol::Windows)
        } else if cfg!(target_os = "macos") {
            symbols.with(TargetSymbol::Macos)
        } else if cfg!(target_os = "ios") {
            symbols.with(TargetSymbol::Ios)
        } else if cfg!(target_os = "linux") {
            symbols.with(TargetSymbol::Linux)
        } else {
            symbols
        }
    }

    /// The defined names in declaration order, joined by `,`. Stable across runs, so it can key
    /// caches.
    pub fn key(self) -> String {
        TargetSymbol::ALL
            .into_iter()
            .filter(|symbol| self.defines(*symbol))
            .map(TargetSymbol::name)
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl Default for CfgSymbols {
    fn default() -> CfgSymbols {
        CfgSymbols::host_debug()
    }
}
