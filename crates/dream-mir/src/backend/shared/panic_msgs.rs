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

/// A capturing closure handed to a `@c` extern's plain `fun` parameter.
pub fn c_capturing_closure(import: &str) -> String {
    format!(
        "panic: a capturing closure was passed to '@c' extern '{import}' as a C function pointer; \
         wrap it in NativeCallback"
    )
}

/// A `fun` value whose target has no C wrapper for the parameter's signature.
pub fn c_no_direct_target(import: &str) -> String {
    format!(
        "panic: '@c' extern '{import}' received a function its C signature cannot call directly; \
         pass a named function or wrap it in NativeCallback"
    )
}

/// `NULL` where a non-`Option` `string` crosses from C.
pub fn c_null_string(what: &str) -> String {
    format!("panic: {what} is NULL where 'string' was declared; declare it 'Option<string>'")
}

pub fn c_result_what(import: &str) -> String {
    format!("the result of '@c' extern '{import}'")
}

pub fn c_callback_arg_what(index: usize) -> String {
    format!("argument {} of a C callback", index + 1)
}

/// Every `@c` boundary message the module's imports can raise.
pub fn c_boundary(mir: &crate::Mir) -> Vec<String> {
    use dream_hir::CShape;
    let mut out = Vec::new();
    for imp in mir.imports.iter().filter(|i| !i.c_params.is_empty() || i.c_ret != CShape::Void) {
        out.push(c_capturing_closure(&imp.name));
        out.push(c_no_direct_target(&imp.name));
        out.push(c_null_string(&c_result_what(&imp.name)));
        for s in &imp.c_params {
            if let CShape::Func { params, .. } | CShape::Callback { params, .. } = s {
                for i in 0..params.len() {
                    out.push(c_null_string(&c_callback_arg_what(i)));
                }
            }
        }
    }
    out
}
