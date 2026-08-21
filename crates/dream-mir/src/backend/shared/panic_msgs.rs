pub const INDEX_OUT_OF_BOUNDS: &str = "panic: index out of bounds";
pub const DIVIDE_BY_ZERO: &str = "panic: attempt to divide by zero";
pub const INVALID_CAST: &str = "panic: invalid cast";
/// Reading an `unowned` field whose referent has already been deallocated.
pub const UNOWNED_NULL_DEREF: &str = "panic: access to deallocated 'unowned' reference";

/// Every panic message base, in a fixed order.
pub const ALL: [&str; 4] = [
    INDEX_OUT_OF_BOUNDS,
    DIVIDE_BY_ZERO,
    INVALID_CAST,
    UNOWNED_NULL_DEREF,
];
