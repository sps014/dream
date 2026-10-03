//! Ownership attributes: who releases what across an extern boundary. Sema, the `@cpp` bridge and
//! MIR read these through the typed helpers below, never by attribute name.

use super::*;

pub const CONSUMING: &str = "consuming";
pub const OWNED: &str = "owned";

pub(super) const SPECS: &[AttributeSpec] = &[
    AttributeSpec {
        name: CONSUMING,
        targets: &[AttributeTarget::ExternFunction],
        args: ArgShape::None,
        repeatable: false,
        doc: "Marks an extern whose reference parameters transfer ownership to the host, which must release them. Extern parameters are otherwise passed borrowed, since a host cannot release a Dream reference.",
    },
    AttributeSpec {
        name: "allow_cycle",
        targets: &[AttributeTarget::Struct],
        args: ArgShape::None,
        repeatable: false,
        doc: "Allows a class to participate in a reference cycle (ARC will not free it automatically).",
    },
    AttributeSpec {
        name: OWNED,
        targets: &[AttributeTarget::ExternFunction],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 0,
            max: 1,
        },
        repeatable: false,
        doc: "The extern's result transfers ownership to Dream. On a `@cpp` member returning `T*`, the Dream object deletes it. On a `@c` extern returning `OwnedCPtr`, `@owned(\"free_fn\")` names the C function that frees the pointer when the last reference is released.",
    },
];

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
