pub const INDEX_OUT_OF_BOUNDS: &str = "panic: index out of bounds";
pub const DIVIDE_BY_ZERO: &str = "panic: attempt to divide by zero";
pub const INVALID_CAST: &str = "panic: invalid cast";
pub const ADD_OVERFLOW: &str = "panic: attempt to add with overflow";
pub const SUB_OVERFLOW: &str = "panic: attempt to subtract with overflow";
pub const MUL_OVERFLOW: &str = "panic: attempt to multiply with overflow";
pub const NEG_OVERFLOW: &str = "panic: attempt to negate with overflow";
pub const DIV_OVERFLOW: &str = "panic: attempt to divide with overflow";
pub const REM_OVERFLOW: &str = "panic: attempt to calculate the remainder with overflow";
pub const SHL_OVERFLOW: &str = "panic: attempt to shift left with overflow";
pub const SHR_OVERFLOW: &str = "panic: attempt to shift right with overflow";
/// Reading an `unowned` field that was never assigned.
pub const UNOWNED_NULL_DEREF: &str =
    "panic: read of unset 'unowned' field (referent not assigned, or already deallocated)";
/// Reading an `unowned` field after the referent was destroyed (slot poisoned on destroy).
pub const UNOWNED_DESTROYED: &str =
    "panic: 'unowned' reference used after its target was destroyed";

/// Every panic message base, in a fixed order.
pub const ALL: [&str; 13] = [
    INDEX_OUT_OF_BOUNDS,
    DIVIDE_BY_ZERO,
    INVALID_CAST,
    ADD_OVERFLOW,
    SUB_OVERFLOW,
    MUL_OVERFLOW,
    NEG_OVERFLOW,
    DIV_OVERFLOW,
    REM_OVERFLOW,
    SHL_OVERFLOW,
    SHR_OVERFLOW,
    UNOWNED_NULL_DEREF,
    UNOWNED_DESTROYED,
];
