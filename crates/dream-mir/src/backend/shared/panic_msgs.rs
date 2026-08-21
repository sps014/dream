pub const INDEX_OUT_OF_BOUNDS: &str = "panic: index out of bounds";
pub const DIVIDE_BY_ZERO: &str = "panic: attempt to divide by zero";
pub const INVALID_CAST: &str = "panic: invalid cast";
/// Reading an `unowned` field that was never assigned.
pub const UNOWNED_NULL_DEREF: &str =
    "panic: read of unset 'unowned' field (referent not assigned, or already deallocated)";
/// Reading an `unowned` field after the referent was destroyed (slot poisoned on destroy).
pub const UNOWNED_DESTROYED: &str =
    "panic: 'unowned' reference used after its target was destroyed";

/// Every panic message base, in a fixed order.
pub const ALL: [&str; 5] = [
    INDEX_OUT_OF_BOUNDS,
    DIVIDE_BY_ZERO,
    INVALID_CAST,
    UNOWNED_NULL_DEREF,
    UNOWNED_DESTROYED,
];
