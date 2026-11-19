//! Stack growth for the compiler's recursive tree walks.

/// Grows the thread stack before a recursive step runs low. A debug build spends far more stack
/// per level than the parser's nesting limit assumes, and flat chains (`a + b + …`,
/// `x.f().f()…`) are not nested at all, so every tree walk that recurses with the input calls
/// this instead of relying on the thread's initial stack.
pub fn grow<R>(f: impl FnOnce() -> R) -> R {
    stacker::maybe_grow(256 * 1024, 8 * 1024 * 1024, f)
}
