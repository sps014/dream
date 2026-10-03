//! Target pointer width and Future layout.

use crate::abi::TargetAbi;
use dream_abi::target::TargetSpec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Llvm(TargetSpec),
}

impl Target {
    pub fn native() -> Self {
        Self::Llvm(TargetSpec::host())
    }

    pub fn wasm32() -> Self {
        Self::Llvm(TargetSpec::wasm32())
    }

    pub fn spec(&self) -> &TargetSpec {
        let Self::Llvm(spec) = self;
        spec
    }

    pub fn abi(&self) -> TargetAbi {
        TargetAbi::for_target(self.spec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_layout_uses_selected_pointer_width() {
        let narrow = Target::Llvm(TargetSpec::parse("i686-unknown-linux-gnu").unwrap());
        let wide = Target::Llvm(TargetSpec::parse("x86_64-unknown-linux-gnu").unwrap());
        assert_eq!(
            (narrow.abi().future.waker, narrow.abi().future.slots),
            (16, 72)
        );
        assert_eq!(
            (wide.abi().future.waker, wide.abi().future.slots),
            (24, 104)
        );
        assert_eq!(Target::wasm32().abi(), TargetAbi::WASM32);
    }
}
