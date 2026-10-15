//! Ownership attributes: who releases what across an extern boundary. Sema, the `@cpp` bridge and
//! MIR read these through the typed helpers below, never by attribute name.

use super::*;

pub const CONSUMING: &str = "consuming";
pub const ALLOW_CYCLE: &str = "allow_cycle";

pub const OWNED: &str = "owned";

/// True when an extern's reference parameters transfer ownership to the host (`@consuming`).
pub fn is_consuming(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, CONSUMING)
}

/// What an extern's `@owned` says about its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnedResult<'a> {
    /// No `@owned`: the host keeps ownership of what it returns.
    Borrowed,
    /// Bare `@owned`: Dream owns the result and frees it the binding's own way (`delete` for `@cpp`).
    Owned,
    /// `@owned("free_fn")`: Dream owns the result and passes it to the named C function to free it.
    FreedBy(&'a str),
}

impl OwnedResult<'_> {
    pub fn is_owned(self) -> bool {
        self != OwnedResult::Borrowed
    }
}

pub fn owned_result(attributes: &[AttributeNode]) -> OwnedResult<'_> {
    if !has_named_attr(attributes, OWNED) {
        return OwnedResult::Borrowed;
    }
    match named_attr_string_arg(attributes, OWNED, 0) {
        Some(free) => OwnedResult::FreedBy(free),
        None => OwnedResult::Owned,
    }
}

/// Explicit acceptance of possible strong-cycle leaks; it never changes runtime ARC.
pub fn allows_cycle(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, ALLOW_CYCLE)
}
