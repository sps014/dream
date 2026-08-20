//! Pointer width and Future layout for the C emitter.

use crate::abi::TargetAbi;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CTarget {
    Native,
    Wasm32,
}

impl CTarget {
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
