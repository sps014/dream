//! `double` bit reinterpretations. LLVM has no intrinsic for these (they are a `bitcast`), so they
//! cannot be spelled with `@llvm`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatOp {
    /// Reinterprets a `double` as the `long` with the same bits.
    ToBits,
    /// Reinterprets a `long` as the `double` with the same bits.
    FromBits,
}

impl FloatOp {
    pub(super) fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "f64_to_bits" => FloatOp::ToBits,
            "f64_from_bits" => FloatOp::FromBits,
            _ => return None,
        })
    }
}
