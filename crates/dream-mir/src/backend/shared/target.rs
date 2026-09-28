//! Target pointer width and Future layout.

use crate::abi::TargetAbi;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Native,
    Wasm32,
}

impl Target {
    pub fn abi(self) -> TargetAbi {
        match self {
            Self::Native => TargetAbi::native(),
            Self::Wasm32 => TargetAbi::WASM32,
        }
    }

    pub fn is_wasm32(self) -> bool {
        matches!(self, Self::Wasm32)
    }
}
